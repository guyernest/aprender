---
phase: "14"
slug: "laya-training-mcp-server-s3-dataset-in-apr-and-reports-out"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-10-29"
---

# Phase 14: Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Audited 2026-10-29 by `gsd-security-auditor` (ASVS level 1, block_on high): 82 register rows
> (T-14-01..T-14-77 plus the shared supply-chain id T-14-SC on plans 03, 04, 05, 06, 07), every row
> disposition `mitigate`, 82 CLOSED, 0 OPEN. The auditor had no AWS credentials; every live claim rests
> on the committed evidence files (`14-*-EVIDENCE.json`), which the orchestrator wrote or recorded, and
> says so where it matters.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| MCP client to pmcp.run request function | OAuth (M2M) in front of `train`, `train_status`, `validate_dataset`; the function role holds no spend right until the grant | tool arguments, bearer tokens |
| Request function to AWS (DynamoDB, Step Functions, S3) | Inline policy `aprender-laya-train-dev`, attached only after the 401 is observed | dataset URIs, task records, presigned links |
| Owner dataset to S3 (`datasets/<prefix>/`) | Owner credentials write; server roles hold no write on `datasets/*` | training data |
| Step Functions to SageMaker / ECS (Train, PackVerify, Finalize) | Per-principal scoped roles, image digests pinned in the ASL | run inputs, outputs under `runs/<name>/<task>/` |
| Run outputs to the owner's machine | `laya-fetch` re-hashes every manifest entry before landing | `.apr`, reports, manifest |
| Operator shell to AWS | `ze-kasher-dev` only, approval variables per paid or privileged recipe | credentials, state files under `models/laya-trainer/` (gitignored) |

---

## Threat Register

Evidence paths: `lt/` = `crates/aprender-mcp-laya-train/src/`, `ll/` = `crates/aprender-mcp-laya-train-lambda/src/`,
`stack.ts` = `deploy-extensions/lib/laya-trainer-stack.ts`, `ops.py` = `scripts/laya_deploy/laya_trainer_ops.py`,
`run_step.py` = `scripts/laya_train/container/run_step.py`, `lfs.py` = `scripts/laya_deploy/laya_failure_surface.py`,
`drift.py` = `scripts/laya_deploy/laya_drift.py`, `stack_check.py` = `scripts/laya_deploy/laya_trainer_stack_check.py`. Live items cite the evidence file key.

| Threat ID | Severity | Status | Evidence |
|-----------|----------|--------|----------|
| T-14-01 | high | closed | `lt/lib.rs` `check_name`, called before any I/O; live `refusals.bad-name` |
| T-14-02 | high | closed | `lt/lib.rs` `parse_dataset_uri` (bucket equality, `datasets/` prefix, 4 segments, `segment_ok`); live `other-bucket` refused `dataset-uri` |
| T-14-03 | high | closed | `lt/lock.rs` CAS acquire, holder-only release; live `single_flight.second_execution_created=false` |
| T-14-04 | medium | closed | links minted at read time in `status`; live `result_has_no_urls` |
| T-14-05 | low | closed | `lt/finalize.rs` `payload_size` |
| T-14-56 | medium | closed | `attach_result_if_absent` CAS |
| T-14-59 | high | closed | release and compensate only on confirmed not-started (`lt/lib.rs`) |
| T-14-60 | medium | closed | run spec hashed; exact 9-key ASL input asserted; `inputs-changed` in `run_step.py` |
| T-14-61 | medium | closed | `verdict_from_report` test over the real tiny report |
| T-14-06 | high | closed | `bound_check` first; HeadObject pre-bounds plus stream cap (`ll/dataset.rs`) |
| T-14-07 | high | closed | 89-case shared table, 6 mutations RED; Rust and `data.py` replays green at HEAD |
| T-14-08 | medium | closed | `refusal_details_never_echo_row_text` |
| T-14-SC (03) | high | closed | `uv.lock` byte-identical to the owner-approved candidate; `candidate_check_rc 0` |
| T-14-09 | medium | closed | `darwin_unchanged`, equal freeze hashes |
| T-14-10 | high | closed | `guard_link_key`, narrow IAM in `stack.ts` |
| T-14-11 | medium | closed | exact key set asserted in `ll/sfn.rs` |
| T-14-12 | high | closed | `SfnCancelSink`; durable via the reconcile cancel-retry; live cancel ABORTED, job Stopped in 79 s |
| T-14-62 | high | closed | `classify_start_error` default Ambiguous plus `confirm` |
| T-14-13 | medium | closed | `monorepo_invariants.rs` register 11 (D-22), test green |
| T-14-SC (04) | high | closed | `aws-sdk-sfn` 1.108.0 with registry checksum; lock gained packages only (see note 1) |
| T-14-14 | high | closed | deploy template declares no IAM; `check_no_iam`; live pre-grant simulation implicitDeny x3 |
| T-14-15 | medium | closed | only server roles write `runs/*`; `pack.accepted` conjunction in `run_step.py` |
| T-14-16 | medium | closed | `settle_record`, `put_if_different`, first-manifest adoption; `fin_duplicate`, `fin_first_writer_wins` |
| T-14-57 | medium | closed | attach on Cancelled; live cancel result cancelled |
| T-14-17 | high | closed | compute gate before outputs and lock; lock released last; live lock released after compute stopped (see AR-1) |
| T-14-18 | medium | closed | default `expire`, `keep` only for the winner; live tags keep x4 for the passing run |
| T-14-19 | low | closed | `fin_foreign_machine` |
| T-14-63 | high | closed | reconcile sweep (re-drive, 900 s grace, stop rogue executions); live lost-event recovery |
| T-14-64 | medium | closed | owner mismatch gives integrity, no write, no release; alarm in `stack.ts` |
| T-14-SC (05) | high | closed | `aws-sdk-ecs` 1.133.1 in the owner's `names_verified` (see note 2) |
| T-14-20 | high | closed | `DEVICE_RE` kill at first non-cuda device; `cpu-fallback` selftest |
| T-14-21 | high | closed | base tar sha256 pin before extraction plus `safe_extract` |
| T-14-22 | medium | closed | `HF_HUB_OFFLINE`, `TRANSFORMERS_OFFLINE` in `Dockerfile.train` |
| T-14-23 | medium | closed | SIGTERM deadline and outcome-last; live probe grace 118 s (see AR-5) |
| T-14-65 | medium | closed | `REFUSED inputs-changed` selftest cases |
| T-14-SC (06) | high | closed | base, uv and aws zip pinned by digest or sha; `uv sync --frozen`; apt unpinned (AR-3) |
| T-14-24 | high | closed | constant pack binary, stripped child env, path-only argv |
| T-14-25 | medium | closed | `fetch_verified` re-hash; base pin |
| T-14-26 | high | closed | `.apr` uploaded only if accepted |
| T-14-66 | low | closed | build context list; `context_clean true` |
| T-14-SC (07) | high | closed | pack image bases pinned, `--locked`, `sha256sum -c` |
| T-14-30 | high | closed | per-principal scoping; synthesized template has no `*` action; 205 assertions, 41 mutations |
| T-14-31 | high | closed | instance, MaxRuntime 21600 and digests as ASL literals |
| T-14-58 | low | closed | digests copied into `manifest.json` |
| T-14-32 | high | closed | BLOCK_ALL, enforceSSL, S3-managed encryption; ListBucket `StringLikeIfExists` deployed and read back live |
| T-14-33 | medium | closed | tag-filtered `runs/` expiry, none on `datasets/` or `base/`; RETAIN |
| T-14-34 | medium | closed | immutable ECR tags |
| T-14-67 | medium | closed | ECR expires untagged only |
| T-14-68 | high | closed | retry 4 plus DLQ, 10-minute sweep, FinalizerErrors alarm; recovery seen live |
| T-14-69 | medium | closed | `PutObjectTagging` on `runs/*` for the SageMaker and pack roles |
| T-14-35 | high | closed | profile and env refused before any call |
| T-14-70 | medium | closed | non-empty dataset prefix refused (operator script only, AR-4) |
| T-14-36 | high | closed | exact per-probe spend variable, MaxRuntime 1800, one instance; no job left InProgress |
| T-14-37 | medium | closed | base size and sha pin, file allowlist, `--expect` |
| T-14-38 | medium | closed | clean tree, commit-tagged image, digest read back from ECR |
| T-14-39 | low | closed | state dir must be git-ignored (residual W-2) |
| T-14-40 | high | closed | no IAM at publish; revoke before redeploy; live `auth_window_s 10114` with no spend right (AR-2) |
| T-14-41 | high | closed | token only from env, refused in argv (`train_call.py`, `m2m_token.py`) |
| T-14-42 | high | closed | package-in-build-log check; crates root restored byte-identical |
| T-14-43 | medium | closed | profile, account gate and approval variables |
| T-14-71 | high | closed | grant takes no URL; endpoint bound by host label and `/health` |
| T-14-72 | medium | closed | redirects refused |
| T-14-73 | high | closed | revoke must re-read NoSuchEntity |
| T-14-27 | high | closed | `laya_fetch.py` size and sha256 per entry, staged then renamed |
| T-14-28 | high | closed | `check_entry_path`, `dest_for` |
| T-14-29 | medium | closed | `check_dest`, staging removed on failure |
| T-14-74 | medium | closed | `validate_tree` before any download |
| T-14-75 | high | closed | fixed `MUST_KEYS`, no network imports in the evaluator |
| T-14-44 | high | closed | `owner_go` recorded before the paid calls |
| T-14-45 | medium | closed | both probes terminal, zero InProgress |
| T-14-46 | low | closed | evidence uses `<acct>` and `<BucketName>` (residual W-2) |
| T-14-47 | high | closed | unauthenticated `validate_dataset` refused; nothing minted (see W-4) |
| T-14-48 | high | closed | 401 check runs inside the grant's own invocation; no grant timestamp recorded |
| T-14-49 | high | closed | M2M file outside the repo, mode 600 (see W-1: closure is procedural) |
| T-14-76 | high | closed | simulation implicitDeny x3 before the grant, allowed x3 after |
| T-14-77 | medium | closed | table count, lock record and executions unchanged across the four refused trains |
| T-14-50 | high | closed | single-flight and training cancel proven live; packing-cancel leg inconclusive (AR-1) |
| T-14-51 | medium | closed | declaration committed before attempt 1 |
| T-14-52 | medium | closed | fetch verified; local verify refusal recorded, nothing deployed |
| T-14-53 | high | closed | evaluator verdict not-reproduced, `next_trusted false`, result not overstated |
| T-14-54 | high | closed | Laya contracts unchanged through Phase 14 |
| T-14-55 | medium | closed | attempt cap 2, one attempt run |
| T-14-78 | high | closed | `ll/finalizer.rs` SURFACE_AXES with listing and absent_listed axes (3451); I2 oracle (4005-4026); `failure_surface_sweep_clean` (4454); `failure_surface_transient_faults` (4553); fix `drop_absent_entries` (573, called 1069) |
| T-14-79 | medium | closed | I3 oracle (`ll/finalizer.rs` 4028-4072) and I8 `delivery_findings` (4263) swept over trigger x outcome; tracer `failure_surface_tracer_wr01` (4337); outcome-first `finalize_run` (`lt/finalize.rs` 534-545) |
| T-14-80 | medium | closed | I4 `hostile_leaks` over every caller surface, four planted constants (`ll/finalizer.rs` 3380-3392, 4074-4077); `lfs.py --check` sink rules (495-516) bind 26 sink groups (102 hits) |
| T-14-81 | medium | closed | sweep asserts ran == axis product (`ll/finalizer.rs` 4483-4484), no stale entry or unpinned violation (4529-4542); `lfs.py` binds SURFACE_AXES, CATCH_FED, KNOWN_DEFECTS to the artifact (283-296, 485) |
| T-14-SC (16) | high | closed | no manifest or lock change in 468b55b18..HEAD |
| T-14-82 | high | closed | `drop_absent_entries` takes one listing before the decision (`ll/finalizer.rs` 573-603, 1069); tag targets intersected with a post-write listing (963-964); `fin_listed_but_absent_*`, `fin_adopted_manifest_with_absent_entry`; bounded `dropped_note` (`lt/finalize.rs` 89-113); M-WR02-a..e |
| T-14-83 | medium | closed | outcome decides the class (`lt/finalize.rs` 534-545); stage error is detail via `with_stage` (316); `NOTE_STAGE_ERROR_OVERRIDDEN` reaches verdict.json (386-390; `fin_override_note_reaches_verdict`); M-WR01-a..d |
| T-14-84 | medium | closed | `lfs.py` mutation anchors occur exactly once (801-805); `finally` restores and re-reads (745-751, 814-815); end-of-run byte comparison (830-834); 17/17 run 2026-10-30 with the tree unchanged |
| T-14-SC (17) | high | closed | Cargo.toml and Cargo.lock unchanged |
| T-14-85 | medium | closed | three boundaries: `StatusPayload::to_value` redacts error, pack line, notes (`lt/layout.rs` 567-586); `RedactingBackend` (`lt/redact.rs` 359-425, wired `ll/bin/bootstrap.rs` 78); code-only `start_error_label` (`ll/sfn.rs` 55, 191); Catch Cause reduced by `stage_error` (`lt/redact.rs` 330). SK-12 (bucket-name account id in run_prefix/links) excluded by decision as a caller handle |
| T-14-86 | low | closed | accepted (AR-6); partly mitigated: HOSTILE_UNLISTED under a non-whitelisted Cause key (`ll/finalizer.rs` 3390, 3430; M-WR03-c) |
| T-14-87 | medium | closed | M-WR03-a..h (`lfs.py` 197-244) each turn a named table test or I4 red; must-redact / must-keep tables (`lt/redact.rs` 569, 583) |
| T-14-SC (18) | high | closed | no dependency added |
| T-14-88 | medium | closed | synth writes the sidecar (justfile:1417); `check_provenance` (`ops.py` 738-763) refuses sha mismatch, dirty build, non-ancestor, changed or dirty closure; `cmd_infra` refuses before any cdk call (937-947); real-git table (2405-2485) |
| T-14-89 | medium | closed | `check_surface_binding`, one rule per field (`stack_check.py` 519-559); 11 SURFACE_CASES (845-858); 52/52 |
| T-14-90 | low | closed | closure-scoped `git diff --name-only <build> HEAD -- <closure>` (`ops.py` 706-710); `unrelated-commit` case accepted (2449-2452) |
| T-14-SC (19) | high | closed | no package added |
| T-14-91 | high | closed | `drift.py` 412-445: dirty closure refused before build; binary path from cargo's artifact message, hashed; 14-DRIFT-LOCAL-EVIDENCE.json binary sha256 64baa3f2…, closure_dirty false; positive control reproduced |
| T-14-92 | high | closed | `drift.py` template 843-1034: `set -euo pipefail` first, one drift prefix (955), sources read-only, empty-prefix refusal (963-966, accepts aws-cli v2 `None` since 5353b8727), path guards (958-960); static table and bashrs-gate before write (1173-1186) |
| T-14-93 | medium | closed | `scrub` / `assert_clean` (`drift.py` 1295-1313, 1645-1652); evidence scan clean |
| T-14-94 | high | closed | `git diff --quiet 468b55b18 HEAD -- contracts crates/aprender-decide` rc 0 |
| T-14-103 | medium | closed | copy-object `--tagging-directive REPLACE --tagging laya-retention=expire` (`drift.py` 971); tagged put-object (976, 978); tag read-back before `run-task` (980-986); pack uploads tagged (`run_step.py` 88, 415) |
| T-14-SC (20) | high | closed | standard library only |
| T-14-95 | high | closed | T-14-92 / T-14-103 controls on the real generated file; pack LAYA_RUN_PREFIX is the drift prefix (`drift.py` 985); tracer stage ran first |
| T-14-96 | medium | closed | accepted (AR-7); in code: approval variable (`drift.py` 1009), FARGATE (986), 1-5 repeats per argument (1014); realized: exactly 6 tasks, all self-stopped, longest ~17 min (14-DRIFT-EVIDENCE.json) |
| T-14-97 | high | closed | D-15 reply `amend-per-study` in 14-DRIFT-EVIDENCE.json owner_replies (recorded_by orchestrator); contracts and decide crate equal HEAD |
| T-14-98 | medium | closed | same scrub as T-14-93; 14-DRIFT-EVIDENCE.json and 14-DRIFT-DECISION-BRIEF.md scan clean |
| T-14-SC (21) | high | closed | no package installed |
| T-14-99 | high | closed | saved diff: 1 resource (Finalizer Code.S3Key), 0 IAM sections; 14-GAP-REDEPLOY-EVIDENCE.json finalizer.equal true (fc796b8b…3fab, sidecar commit 275ad9611); provenance verify OK at HEAD |
| T-14-100 | high | closed | `cmd_deploy` revokes the grant before publishing (`ops.py` 1579; `revoke_grant` 1500); `cmd_grant` runs the auth-refusal check immediately before `put-role-policy` (1701, 1715); B0-B5 evidence, first 401 23:35:22Z, then GRANT OK |
| T-14-101 | medium | closed | gitignored helper passes the presigned URL to curl on stdin (`-K -`), never prints it; `rm -f -- "${PKG:?}"` in an EXIT trap; package absent; BASHRS GATE PASS |
| T-14-102 | high | closed | owner_replies 11/11 (14-GAP-REDEPLOY-EVIDENCE.json) and 4/4 (14-DRIFT-EVIDENCE.json) recorded_by orchestrator; every aws step in the runbook is a bang-prefixed owner command (procedural closure, as T-14-49) |
| T-14-SC (22) | high | closed | no package installed; `laya-trainer-synth` refuses without cargo-zigbuild (justfile:1406) |

Notes on closures:

1. T-14-SC (04): the same commit re-pointed about 10 existing dependency edges to versions already in `Cargo.lock`
   (`socket2` 0.5.10 to 0.6.4 on a non-Windows runtime path). "The lock only grew" is exact at package level only.
2. T-14-SC (05): `aws-sdk-sagemaker` 1.210.0 was also added. RESEARCH audits it as acceptable, but it is not in the
   owner's `names_verified` list.

*Status: open · closed · open — below high threshold (non-blocking)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-1 | T-14-50, T-14-17 | The live stop of a genuinely running pack container was not observed: the pack task of the gate-failed tweet-stance-16 run exited by itself 13 s in, in the same second as the cancel. Training-side stop and lock-held-until-terminal are proven for both legs; the code path (`stop_live_compute`, `fin_cancel_packing`) is unit-tested. Closes with a gate-passing run cancelled while its pack task runs. | owner (14-14 checkpoint: "accept as inconclusive") | 2026-10-29 |
| AR-2 | T-14-40, T-14-47 | The function is published without OAuth by design (cargo-pmcp cannot render OAuth for pmcp.run) and held no spend right: window 10114 s upper bound, role simulation implicitDeny x3. The true enable time was not observed. | recorded in 14-13; not separately decided | 2026-10-29 |
| AR-3 | T-14-SC (06) | apt packages in the train image are unpinned (declared in the Dockerfile header); the image digest is the identity. | recorded, not separately decided | 2026-10-29 |
| AR-4 | T-14-70 | Dataset-prefix immutability is enforced by the operator script only; server roles hold no write on `datasets/*`. | recorded, not separately decided | 2026-10-29 |
| AR-5 | T-14-23 | The container SIGTERM path itself was not exercised live (the cancelled job never started its container); the probe measured the grace (118 s) only. | recorded, not separately decided | 2026-10-29 |
| AR-6 | T-14-86 | `caller_safe` is a blacklist over job-written trainer and packer lines, so an unknown shape can still reach `error` or `notes`. The backend and dispatch sinks are allowlists, and the Catch Cause is reduced to whitelisted keys (HOSTILE_UNLISTED proves it). Recorded in 14-FAILURE-SURFACE.md. | owner, 2026-10-30 (secure-phase gate) | 2026-10-30 |
| AR-7 | T-14-96 | The drift script has no total task cap and its 45-minute cap stops only the watcher (no stop-task). The executed study ran exactly six tasks, all self-stopped, longest ~17 min. Any re-run is owner-run behind LAYA_DRIFT_APPROVED=graviton. Follow-up: a total cap plus stop-task. | owner, 2026-10-30 (secure-phase gate) | 2026-10-30 |

---

## Unregistered Findings (not threat register rows)

| Id | Severity | Finding | Action |
|----|----------|---------|--------|
| W-1 | warning | An untracked `.env.m2m` sits in the repo root and is not gitignored. Not read (a secret-read hook blocks it). It may be a leftover copy of the Phase 14 M2M credential or an older one. | Owner: confirm what it holds; if a live `client_secret`, move or delete it and rotate; add an ignore pattern; do not `git add -A`. No tracked file holds a credential. |
| W-2 | low | `14-08-SUMMARY.md` names a real AWS account id (the shell's default profile account, not `ze-kasher-dev`). Account ids are not secrets, but this sits against T-14-39's intent. | Redact before any upstream PR. |
| W-3 | low | The finalizer asset is not byte-reproducible across synths; the deployed `7288da6d...` is a rebuild of unchanged source (tests 60/60 on that source). Every later synth re-uploads it. | Accepted by the orchestrator in 14-IAM-FIX-NOTE.md; note for future deploys. |
| W-4 | info | During the open window anonymous `initialize` and `tools/list` answered 200, disclosing the three tool names. Calls were refused; nothing was minted. | none |
| W-5 | warning | The real account ids of ze-kasher-dev and augury-dev are in the committed `justfile` (target-account gate table and its case table) and `scripts/laya_gates.tsv`, added in 13-04; the gap round added none of these lines. Same class as W-2. | Parameterize or redact before any upstream PR, or record as accepted. |
| W-6 | info | 14-22 B2 attempt 1 revoked the grant, then failed on an expired pmcp.run login before any upload (safe direction; maps to T-14-100). `_laya-crates-root-swap` mislabels its swap line as "decide config" (cosmetic, fails closed). | Follow-up: pmcp.run login precheck before the revoke; relabel the swap recipe and its selftest fixture together. |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-29 | 82 | 82 | 0 | gsd-security-auditor (verdict SECURED; offline re-runs at HEAD: 93 + 60 crate tests, 15 invariants, 205 stack assertions with 41 mutation cases, all Python selftests) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-29; gap round T-14-78..103 verified 2026-10-30 (31 mitigated, 2 accepted: AR-6, AR-7)

## Security Audit 2026-10-31

| Metric | Count |
|---|---|
| Threats found | 33 |
| Closed | 33 |
| Open | 0 |
