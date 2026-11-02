---
phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
verified: 2026-11-02T16:00:00Z
status: passed
score: 5/5 must-haves verified
covered_files:
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-01-PLAN.md"
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-01-SUMMARY.md"
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-02-PLAN.md"
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-02-SUMMARY.md"
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-03-PLAN.md"
  - ".planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-03-SUMMARY.md"
  - "scripts/laya_deploy/laya_control.py"
  - "scripts/laya_deploy/laya_margin_diagnosis.py"
  - "scripts/laya_deploy/laya_repro_eval.py"
covered_digest: "v4:sha256:39e63f29c45d56515df26daeb8d840ef659701c45416e82809a3e1370a10c20b"
behavior_unverified: 0
overrides_applied: 0
re_verification: false
---

# Phase 16: Laya pipeline-equivalence control (antisemitism-v1, CUDA vs Mac) Verification Report

**Phase Goal:** Show whether the SageMaker CUDA training pipeline reproduces the Mac (MPS) Phase 10 antisemitism-v1 result on identical inputs and recipe. One owner-run attempt, judged on the three-seed mean margin in the owner-approved band [0.209, 0.269], with the gate passing unwaived (shipped-seed ECE <= 0.10). Claim scope is pipeline equivalence on this task only.
**Verified:** 2026-11-02
**Status:** passed (with disclosed deviations, none of them a goal gap)
**Re-verification:** No, initial verification

Method: the declaration was materialised from its first-add blob (`git show a3d4d2baa:...`) and every verdict-bearing number was recomputed from the fetched files under `models/decide/antisemitism-v1-cuda-ctl`. SUMMARY claims were not relied on. No AWS or paid command was run, and the declaration, evidence and STATE.md were not edited.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | The declaration (antisemitism-v1, recipe `d70c3f3d...`, 4 epochs, seeds 13/17/23, cuda, cap 1, no retry class, no waiver; mean margin in [0.209, 0.269], shipped-seed ece_post in [0.0, 0.10]) is committed ALONE before any staging or paid step; the evaluator is reused unchanged; a rehearsal shows it can pass and fail. | VERIFIED | `a3d4d2baa` touches exactly one file (`16-CONTROL-DECLARATION.json`, 102 insertions) and exactly one commit ever touched that path. Its blob sha256 is `600b6494...f096` and equals the working file (`cmp`). The commit is dated 12:02 -0800 on 2026-11-01; staging replies are at 20:38Z (12:38 -0800) and the first go at 12:42 -0800. `git diff a3d4d2baa HEAD` over `laya_repro_eval.py`, `laya_margin_diagnosis.py`, `laya_eligibility.py`, `scripts/laya_train`, `contracts`, `.github`, the declaration and the fixture is empty. A fresh `laya_control.py rehearse --json` returned both pass rows (`in-band`, `mean-rule`, `edge-low-inside`, `edge-high-inside`) and defect rows (`mean-below-floor`, `mean-above-ceiling`, `ece-over-bar`, `device-mps`, `wrong-recipe`, `changed-input`, `two-seeds`, `pack-refused`). `check-declaration` printed `DECLARATION OK`. |
| 2 | The Phase 10 Mac per-seed numbers are a tracked, text-free fixture with an integrity check against `10-GATE-RUN-EVIDENCE.json`; per-seed CUDA-minus-MPS differences are reported descriptively, never judged. | VERIFIED | Fixture sha256 `d7f19f52...` equals the declaration's `reference_fixture.sha256`. The Phase 10 evidence sha256 `c9ea039a...` equals the declaration's `source_evidence_sha256`. A fresh `paired-report` over the fetched run equals the evidence's `paired` block exactly (`paired equal: True`), and it carries `descriptive_only: true` with no verdict or band key. The evidence's own findings disclose that the instrument prints null for the run side of `best_epoch`, `clamp_hit` and `train_seconds`, and that a `paired_supplement` fills them from `run/variance-report.json`, kept outside the instrument's block. |
| 3 | The dataset is staged once under `datasets/goldstandard-2024/` with Phase 10's digests, and `validate_dataset` accepts 8,510 train and 2,129 eval rows before any paid step. | VERIFIED (owner-run steps through recorded replies, cross-checked by the run's own report) | Local files hash to the declared digests (`33fd8d7c...`, `81a923d4...`, `f4a0cf7c...`). The orchestrator-recorded S0 reply (`STAGE CHECK OK`) and V reply (`VALIDATE CHECK OK rows=8510/2129 ... deferred_rules=0`) precede the first go. I cannot re-observe S3 without AWS. The trainer's own `gate-report.json` records `inputs_sha256` for task, train and eval equal to the declared digests, so the data the run consumed is the declared data. |
| 4 | One owner-run SageMaker CUDA attempt (a per-run go recorded by the orchestrator; compute above 1 h) is fetched, classified by the consumption rule and judged by the committed evaluator over the declaration's first-add blob; CUDA wall time is measured; a pre-verdict stop is not the attempt. | VERIFIED | `check-run-go` printed `RUN-GO OK` for both `16-RUN-GO.json` and `16-RUN-GO-2.json` against the materialised blob and commit `a3d4d2baa...`. `classify-run --run-dir models/decide/antisemitism-v1-cuda-ctl` returned `consumed` ("exit_class is pass: the gate executed"). `outcome.json` has `exit_class pass`, `train_rc 0`, `device_used cuda:0`, `train_seconds 11485.1`. `train.log` line 55 is `GATE PASS`, with no WAIVER line. `evaluate --attempt 1` over the blob returned `class: pass`, `verdict_if_final: reproduced`, every must clause and both band clauses true. Wall time is measured: 11,485.1 s for the job and 1,194 to 1,197 s per seed. Per-seed margins from `gate-report.json` are 0.249666, 0.261325 and 0.258164, so the mean is **0.256385**, inside [0.209, 0.269] (0.0126 under the ceiling). The shipped seed is 17: ECEs 0.0380, 0.0133 and 0.0114 have median 0.0133, which is seed 17 under the median-ECE rule, and ece_post 0.0133 is within [0.0, 0.10]. The pack is `accepted`, `deploy_eligible` true, `waived` null, and the artifact sha256 `22796ff9...` matches the local .apr (846,220,804 bytes). |
| 5 | The evidence states pipeline equivalence on THIS task only, with `next_trusted` false and `sets_next_trusted` false whatever the verdict; it does not reproduce motivation-v1, does not change D-16 and does not lift the Phase 14 server's untrusted state. | VERIFIED | The evidence has `next_trusted: false` and `sets_next_trusted: false`, and the declaration has `sets_next_trusted: false`. The evidence `claim_scope` text is the declaration's. `trust_note` and `outcome_meaning` make no trust claim. The evidence file has no uncommitted changes. `scan-identifiers` over the evidence, amendment, RUN-GO-2 and 16-03-SUMMARY printed `NO IDENTIFIERS`. |

**Score:** 5/5 truths verified. No behavior-dependent truth was left present-but-unexercised: the evaluator, classifier and paired-report were all re-run.

### Independent recomputation summary

| Quantity | My value | Evidence value |
|----------|----------|----------------|
| Three-seed mean margin | 0.25638519753336625 | 0.25638519753336625 |
| Shipped seed / margin / ece_post | 17 / 0.26133 / 0.013292 | 17 / 0.26133 / 0.013292 |
| Evaluator class / verdict | pass / reproduced | pass / reproduced |
| classify-run on the run directory | consumed | consumed |
| classify-run on `...-attempt1` | unresolved | unresolved (recorded) |
| Declaration sha256 / unchanged since `a3d4d2baa` | `600b6494...` / yes | same |
| Amendment sha256 (`73b6d5c94` equals HEAD equals RUN-GO-2) | `7b43aa65...` | same |

### Amendment, RUN-GO-2 and the attempt cap

- **Cap consistency.** The declaration says `attempt_cap: 1`, `retry_classes: []`, and "a run stopped by the 21,600 s runtime cap ... is a timeout and is not an attempt (D-06); re-running it needs the owner's new approval." Run 1 (task `10ea131b`) hit exactly that: `exit_class timeout`, 21,637.8 s, no gate report, no GATE line. The owner's verbatim ruling (recorded by the orchestrator in the amendment `owner_decisions` and `16-RUN-GO-2.json`) is "Attempt 1 can be considered as not an attempt and phase 16 can re-run ... You have my go." Run 2 is therefore the one declared attempt. `attempt_number: 1` with `physical_run: 2`, and `evaluate --attempt 1`, match the declaration. Cap 1 is not exceeded: exactly one gate-executing run exists.
- **RUN-GO-2 is bound correctly.** It carries the declaration commit and sha256 (checked by `check-run-go`) and the amendment commit `73b6d5c94` and sha256 (both recomputed above). Commit order is the reliable ordering: infra `b319fc95d`, amendment `73b6d5c94`, go-2 `5eb4c90c6`, then (owner-run deploy), then the start record `cf6f933aa`. The go was conditional on the deploy, and the deploy reply precedes the second train call.
- **What the amendment changed.** Only infrastructure: runtime cap 21,600 s to 36,000 s and `ml.g5.xlarge` to `ml.g5.4xlarge` (same A10G, 4 to 16 vCPU). Band, inputs, recipe, epochs, seeds, judging statistic and claim scope are untouched, and the evaluator still reads the declaration's first-add blob. The amendment itself notes the limitation that CPU count can change fp32 reduction order, so this is not a bit-identity claim.

### Deviations: real versus cosmetic

**Real (disclosed or discoverable, none moves the verdict or defeats a success criterion):**

1. **Attempt-1 classification rests on an owner ruling, not on the tool.** `classify-run` returns `unresolved` for `exit_class: timeout` (it maps only `refused` and `crash` to pre-verdict). The declaration's `retry.rule` contradicts itself: it names timeout as "not an attempt" but describes the pre-verdict exit classes as refused or crash, and separately says an inconclusive run "is decided by the owner, never counted as not an attempt". The owner's ruling resolves it, the evidence records the disagreement, and the classifier was correctly not changed after the result. The ruling was made with the partial attempt-1 variance report (mean 0.2481, in band) in view. That figure was in-band, so there was no incentive to discard a miss, and the declaration prose and D-06 predate the run. This is a drafting defect of the declaration to disclose, not a goal gap. Worth carrying forward: a future declaration should list `timeout` in the pre-verdict exit classes and fix `classify-run` test-first.
2. **Plan-level re-run protocol was not followed literally, and the evidence's `deviations` does not list it.** 16-03-PLAN prescribed, for a re-run, an `owner_replies` entry with step `rerun-go` (and `unresolved-ruling`), a fresh go file, and a new run name `antisemitism-v1-cuda-ctl-b`. Actual: the ruling and go live in the amendment `owner_decisions` and `16-RUN-GO-2.json` (orchestrator-recorded, verbatim) but not as `owner_replies` steps, and the run name stayed `antisemitism-v1-cuda-ctl`, with the attempt-1 directory renamed by the owner to `...-attempt1` to avoid a fetch collision (recorded in the `F-attempt1` reply). The evaluator reads inputs by sha256 and never the run name, so this does not affect the verdict. It is an undisclosed process deviation of minor weight.
3. **Deployed request Lambda differs from the repo.** The Lambda was not redeployed, so deployed `LOCK_LEASE_SECS` is 39,600 s against 54,000 s in the repo, shorter than the new 50,400 s machine timeout. The owner was told, and this run finished in about 3.2 h plus pack and verify, so there was no effect. It is a latent single-flight lock hazard for any future run longer than 11 h. This is outside Phase 16's goal and is flagged for the owner.
4. **The declaration's `changes_from_reference` and 2-to-6 h estimate are stale for instance type and time.** The declaration is immutable, and the amendment records the superseding facts (the 2 to 6 h estimate was wrong for `ml.g5.xlarge`; the real full run took 3.19 h on `ml.g5.4xlarge`). The measured wall time is recorded in the evidence, so SC4's "wall time is measured" holds. Phase 16 spent two paid runs against a declared cap of one attempt, which the owner authorised.

**Cosmetic:**

5. **`owner_replies` step names `R-attempt2`, `S-attempt2`, `F-attempt1`, `F-attempt2`, `H-attempt2` against a Task 2 verify expecting `R`, `S`, `F`, `H`.** The unedited replies carry the right content (task id, POLL lines, FETCH line with apr sha256, hygiene values), all `recorded_by: orchestrator`. The verify assertion would stop at F as written, but this is a literal-string mismatch only. The evidence lists it under `deviations`.
6. **Scope-check clauses false because of `b319fc95d`.** Verified independently: since `a3d4d2baa`, the only non-planning diffs are `83e48caef` and `97a13be14` (the 16-02 stager pin and `laya_control.py`, whose working sha256 `d35f9b12...` equals the recorded pin), plus `b319fc95d` (layout.rs, failure-surface fixture, deploy-extensions, stack check). Evaluator, diagnosis tool, eligibility tool, `scripts/laya_train`, contracts, `.github`, declaration and fixture are identical to `a3d4d2baa`. The claim in the evidence is accurate.
7. **Timestamp approximations.** `S-/F-/H-attempt2` carry `2026-11-02T00:00:00Z` placeholders that sort before `R-attempt2` (03:33:50Z). `16-RUN-GO-2.recorded_at` (03:30:00Z) postdates its own commit (03:20:50Z). The evidence flags the approximations. Git commit order is the reliable ordering.
8. **Amendment `status` still reads `owner-approved-pending-deploy`.** The file is hash-bound by RUN-GO-2 and was not updated after the deploy. This is consistent with the immutability rule.
9. **Evidence is internally consistent where I probed.** The `pre_verdict_failures` entry for attempt 1, `attempts[0]` and the `attempt_numbering` note all agree with the declaration and amendment.

### Anti-Patterns Found

None. No TBD/FIXME/XXX markers in the 16-0x SUMMARY files; no committed account id, bucket, ARN or host (`scan-identifiers`: `NO IDENTIFIERS`).

### Requirements Coverage

REQUIREMENTS.md holds no entries for this phase; the phase decisions D-01 to D-09 (`16-CONTEXT.md`) map to success criteria 1 to 5 above and are satisfied. D-09 (no waiver): `pilot_waiver` null, `waived` null, `waived=none` in the packed line.

### Human Verification Required

None required for the verdict. Optional owner decisions that this report does not make:

- Whether to treat the declaration/classifier timeout mismatch (deviation 1) as a follow-up fix, and whether to log the process deviation (2) in the evidence `deviations` list (editing the evidence was out of scope for this verification).
- Whether to redeploy the request Lambda so that deployed `LOCK_LEASE_SECS` matches the repo (deviation 3).
- Whether a pass redefines trust. This is a separate owner ruling and the phase deliberately sets nothing.

### Gaps Summary

No gaps. The phase goal is achieved against the committed declaration: the SageMaker CUDA pipeline's three-seed mean margin (0.2564) lies in the owner-approved band [0.209, 0.269], the gate passed unwaived, the pack was accepted and deploy-eligible, `next_trusted` and `sets_next_trusted` are false, and the claim is scoped to this task. The same-seed spread against the unfetched attempt-1 partial report (0.2522/0.2519/0.2403 against 0.2497/0.2613/0.2582) and the all-positive CUDA-minus-MPS margin differences (+0.0139, +0.0155, +0.0221) are descriptive observations the evidence records and the declaration does not judge.

---

_Verified: 2026-11-02_
_Verifier: Claude (gsd-verifier)_
