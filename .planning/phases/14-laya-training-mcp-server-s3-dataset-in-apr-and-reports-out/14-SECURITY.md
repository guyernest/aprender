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
`run_step.py` = `scripts/laya_train/container/run_step.py`. Live items cite the evidence file key.

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

---

## Unregistered Findings (not threat register rows)

| Id | Severity | Finding | Action |
|----|----------|---------|--------|
| W-1 | warning | An untracked `.env.m2m` sits in the repo root and is not gitignored. Not read (a secret-read hook blocks it). It may be a leftover copy of the Phase 14 M2M credential or an older one. | Owner: confirm what it holds; if a live `client_secret`, move or delete it and rotate; add an ignore pattern; do not `git add -A`. No tracked file holds a credential. |
| W-2 | low | `14-08-SUMMARY.md` names a real AWS account id (the shell's default profile account, not `ze-kasher-dev`). Account ids are not secrets, but this sits against T-14-39's intent. | Redact before any upstream PR. |
| W-3 | low | The finalizer asset is not byte-reproducible across synths; the deployed `7288da6d...` is a rebuild of unchanged source (tests 60/60 on that source). Every later synth re-uploads it. | Accepted by the orchestrator in 14-IAM-FIX-NOTE.md; note for future deploys. |
| W-4 | info | During the open window anonymous `initialize` and `tools/list` answered 200, disclosing the three tool names. Calls were refused; nothing was minted. | none |

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

**Approval:** verified 2026-10-29
