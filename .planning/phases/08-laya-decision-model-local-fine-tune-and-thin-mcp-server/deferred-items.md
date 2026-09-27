# Phase 8 — Deferred Items (out-of-scope discoveries)

Logged by executors under the scope-boundary rule: found while verifying a plan, not caused by it,
not fixed by it.

## From plan 08-01

### D-ITEM-08-01-A: CI's strict-test-binding guard is VACUOUS on this branch, and red underneath

- **Found during:** 08-01 Task 1 verify (`bash scripts/check_contract_test_binding.sh`).
- **Symptom:** rc=1, `VACUOUS: strict-test-binding gate was SKIPPED (contract validation failed); nothing was measured.`
- **Cause 1 (the skip):** `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:` section, so
  `pv lint`'s validate gate reports `PROVABILITY-001: Kernel contract has no kani_harnesses
  (spectral-indices-v1)` and every downstream gate is skipped. Introduced by `fdf6b1802`
  ("feat: add spectral indices and aprender-mcp-chronos-lambda ..."), which is on this branch
  and NOT on `origin/main` (`git merge-base --is-ancestor fdf6b1802 origin/main` rc=1). The file
  has no `contract:` key and is not in the Makefile `CONTRACTS` list, so `make contract-validate`
  never sees it.
- **Cause 2 (underneath):** with the skip lifted in a temporary copy (a declared-not-executed
  `KANI-SPECTRAL-001` naming `test_falsify_spectral_001_bounds`, which exists in
  `crates/aprender-image/src/tests.rs`), the guard runs and reports
  `Resolved 585 test references; 44 dangling across 15 contract(s)` and FAILS on two contracts
  outside the baseline: `contracts/chronos-bolt-parity-v1.yaml` (9 dangling, baseline 0) and
  `contracts/setfit-encoder-conformance-v1.yaml` (8 dangling, baseline 0).
- **Phase 8 status:** in both the as-is and the lifted runs, NO Phase 8 contract is named. The four
  Phase 8 contracts cite no Rust test at all (staged binding); the only test reference is
  `FALSIFY-DECIDE-TOOL-009`, which is `LIVE-PENDING` (unbindable by design).
- **Why not fixed here:** the skip fix alone does not turn the guard green (17 pre-existing dangling
  references in Phase 1 / Phase 6 contracts), so it would be a partial repair to two other phases'
  contracts. It belongs in its own change: add `kani_harnesses:` (and a `contract:` key) to
  spectral-indices-v1, then fix or re-cite the 17 dangling references — never raise the baseline.
- **Impact on 08-01 acceptance:** the "prints its PASS line" clause of the strict-binding criterion
  could not be satisfied by this plan; the "does not name the new contract" clause was verified in
  a run where the gate actually measured.

### D-ITEM-08-01-B: README CLI command count is stale (FALSIFY-README-003)

- **Found during:** 08-01 Task 1 (`bash scripts/check_readme_claims.sh`).
- **Symptom:** `FAIL FALSIFY-README-003 cli_command_count: README claims 110, contracts/apr-cli-commands-v1.yaml lists 111 commands`.
- **Not caused by 08-01**, unrelated to contracts. FALSIFY-README-002 (contract count) was ALSO
  failing before 08-01 (README said 1778 in two prose lines against 1791 on disk); 08-01 fixed that
  one because it moves the contract count, and it now passes at 1795.

## From plan 08-03

### D-ITEM-08-03-A: D-ITEM-08-01-A re-measured; still blocks the strict-binding PASS line

- **Found during:** 08-03 Task 1 and Task 2 verify (`bash scripts/check_contract_test_binding.sh`).
- **Symptom:** unchanged: rc=1, `VACUOUS: strict-test-binding gate was SKIPPED (contract validation failed)`,
  because `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:`.
- **Measured around it:** `pv lint --strict-test-binding` was run on a temporary copy of `contracts/` with
  the skip lifted. The copy was a sibling dir inside the repo, because the source scan is rooted at the
  contract dir's parent, and it was deleted right after. Result: 587 refs resolved (585 at 08-01, plus
  this plan's two bindings), and `laya-parity-v1` has **0** dangling references. The other 15 contracts
  and 44 dangling references are identical to D-ITEM-08-01-A. A mutated binding (`tiny_parity_mutant_zz`)
  was reported dangling in the same run, so the resolver discriminates.
- **Why not fixed here:** same reason as D-ITEM-08-01-A. The fix belongs to other phases' contracts.
- **Also found:** `.planning/WINDOWS.md` refuses every append (`Ledger entry 24 has invalid status:
  "resolved"`), so this plan's deviation could not be recorded there. That was not caused by 08-03 and
  is not fixed here.

## From plan 08-04

### D-ITEM-08-04-A: D-ITEM-08-01-A re-measured; the strict-binding PASS line is still unreachable

- **Found during:** 08-04 Task 1 and Task 2 verify (`bash scripts/check_contract_test_binding.sh`).
- **Symptom:** unchanged: rc=1, `VACUOUS: strict-test-binding gate was SKIPPED`, because
  `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:`.
- **Measured around it** (temporary lifted copy `contracts-lift-0804/`, deleted after each run): 593 refs
  resolved (589 after Task 1 — 587 at 08-03 plus the two laya-parity-v1 bindings — and 4 more from the
  decide-apr-v1 legs), **0 dangling in laya-parity-v1 and decide-apr-v1**; the other 15 contracts and 44
  dangling refs are identical to D-ITEM-08-01-A. Mutated binding names were flagged in both Phase 8
  contracts, so the resolver discriminates.
- **Why not fixed here:** same reason as D-ITEM-08-01-A.

### D-ITEM-08-04-B: `cargo fmt --all -- --check` fails on files from `fdf6b1802`

- **Found during:** 08-04 final checks.
- **Symptom:** diffs in `crates/aprender-image/src/{lib,spectral,tests}.rs` and
  `crates/aprender-mcp-chronos/{build.rs,src/lib.rs}` — all from `fdf6b1802` (the commit behind
  D-ITEM-08-01-A). `cargo fmt -p aprender-decide -- --check` exits 0.
- **Why not fixed here:** out of scope (other phases' crates); a one-line `cargo fmt` in its own change.

## From plan 08-05

### D-ITEM-08-05-A: D-ITEM-08-01-A re-measured; the strict-binding PASS line is still unreachable

- **Found during:** 08-05 Task 2 verify 2 (`bash scripts/check_contract_test_binding.sh`).
- **Symptom:** unchanged: rc=1, `VACUOUS: strict-test-binding gate was SKIPPED`, because
  `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:`.
- **Measured around it** (temporary lifted copy `contracts-lift-0805/`, deleted after each run): 622 refs
  resolved (593 at 08-04, plus 28 decide-apr-v1 legs for FALSIFY-001/002/003/004/005/008/009/011 in the
  GREEN commit, plus FALSIFY-006 in the trybuild commit), **0 dangling in decide-apr-v1 and
  laya-parity-v1**; the other 44 dangling refs are identical to D-ITEM-08-01-A. Two mutated binding names
  (`nan_weight_mutant_zz`, `index_capacity_mutant_zz`) were both flagged, so the resolver discriminates.
- **Also:** `.planning/WINDOWS.md` still refuses every append (`Ledger entry 24 has invalid status:
  "resolved"`), so this unrun-verify item is recorded here instead of the ledger.
- **Why not fixed here:** same reason as D-ITEM-08-01-A.

## From plan 08-06

### D-ITEM-08-06-A: D-ITEM-08-01-A re-measured; the strict-binding PASS line is still unreachable

- **Found during:** 08-06 Task 2 verify 1 (`bash scripts/check_contract_test_binding.sh`).
- **Symptom:** unchanged: rc=1, `VACUOUS: strict-test-binding gate was SKIPPED (contract validation failed)`,
  because `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:`.
- **Measured around it** (temporary lifted copy `contracts-lift-0806/`, deleted after the runs): 644 refs
  resolved (622 at 08-05 plus exactly this plan's 22: 20 `aprender-mcp-decide` legs for TOOL-001..008 and the
  two `aprender-decide` stance-order legs TOOL-004 binds), **0 dangling in decide-tool-boundary-v1**; the other
  44 dangling refs are identical to D-ITEM-08-01-A. Two mutated binding names
  (`bounds_match_contract_mutant_zz`, `admission_refuses_over_pending_mutant_zz`) were both flagged, so the
  resolver discriminates on this contract.
- **Why not fixed here:** same reason as D-ITEM-08-01-A.

### D-ITEM-08-06-B: README's layout tree still says "(82 crates total)"

- **Found during:** 08-06 Task 1 (README crate count).
- **Symptom:** README.md line 224 (`└── ... (82 crates total)`) disagrees with the gated metrics row, which this
  plan moved 88 -> 89 from `cargo metadata --no-deps`. `readme_contract` only checks the metrics row, so the
  tree line has drifted unnoticed across several phases.
- **Why not fixed here:** pre-existing drift in prose no gate reads; the plan scoped the edit to the gated count.

## From plan 08-07

### D-ITEM-08-07-A: the bootstrap handler has not run under a Lambda runtime

- **Found during:** 08-07 final verification.
- **What is proven locally:** every piece the handler composes. That covers the stateless loopback
  (identity, cold-first and both maximal shapes over real HTTP), the S3 loader through an injected
  fetcher, `LoadOnce` re-arming after a failure, and the probe-id filter and load-evidence strings.
  The `bootstrap` bin compiles for aarch64 (zigbuild, glibc 2.34), and `resolve_model_future_is_send`
  pins the `Send` property lambda_http needs.
- **What is not:** `main.rs::handler` itself. That includes the 503-on-failed-load path, the
  `x-decide-load` header on a proxied response, and the `decide.load` line in CloudWatch. No
  Lambda runtime emulator (`cargo lambda watch`) was run, and a live deploy belongs to 08-10 and 08-11.
- **Also:** `.planning/WINDOWS.md` still refuses every append (`Ledger entry 24 has invalid status:
  "resolved"`), so this unrun-verify item is recorded here instead.

### D-ITEM-08-07-B: a per-crate deploy root builds the wrong `*-lambda` package

- status: resolved
- **Resolved by:** plan 08-10 (user decision shared-crates-root, 2026-09-26). `just laya-deploy`
  deploys from `crates` with server `aprender-mcp-decide`, and `just laya-resolver-proof` executed
  cargo-pmcp 0.24.3's own resolver on this workspace: root `crates` -> `crates/aprender-mcp-decide-lambda`;
  the per-crate root -> `crates/aprender-mcp-chronos-lambda` (the trap, confirmed). The durable
  upstream fix is D-ITEM-08-10-B.
- **Found during:** 08-07 Task 3, while writing `.pmcp/deploy.toml.template`.
- **Symptom:** cargo-pmcp 0.24.2's `find_lambda_package_dir` tries `<deploy-root>/{server}-lambda`
  first, then falls back to the first workspace `*-lambda` package with a `bootstrap` bin. With
  `crates/aprender-mcp-decide-lambda` as the root, the first branch cannot match. In
  workspace-member order, `cargo metadata --no-deps` lists `aprender-mcp-setfit-lambda` first
  (measured), and in alphabetical order `aprender-mcp-chronos-lambda` comes first. Either way,
  `cargo pmcp deploy --manifest-path crates/aprender-mcp-decide-lambda` would ship another server's
  binary.
- **Owner:** plan 08-10 (its resolver-proof task and deploy-root decision already cover it). The
  template now documents the trap instead of prescribing the per-crate command.

## From plan 08-09

### D-ITEM-08-09-A: the stdio server's real-model leg is DEFERRED (user approval, 2026-09-26)

- status: open
- **Not run:** `APR_MCP_E2E_DECIDE_MODEL=<real .apr> cargo test -p aprender-mcp-decide --release --test e2e_stdio`,
  test `a_real_decide_model_classifies_over_live_stdio` (crates/aprender-mcp-decide/tests/e2e_stdio.rs).
  It would assert one tool, the served identity equal to the file's sha256 (D-11), K probabilities
  summing to 1, and labels from the task, over live stdio MCP.
- **Why:** no artifact that `pack_laya verify` accepts exists (option 3: both D-19 demo runs are
  fail-closed vectors). decide-apr-v1 knows only `production` and `synthetic-fixture`, so a base
  en-root evidence pack has no honest variant. Serving a low-level pack of a fail-closed vector would
  contradict `demo.fail_closed_rule`. The leg is not run on a stand-in.
- **What covers the gap meanwhile:**
  - the tiny-fixture stdio leg in the same test file;
  - `verify_path` loading real-weights bytes from a FILE through `Decider::load_path` (all eight
    rungs, probe replay) with the 280-row re-scores, in `tests/fail_closed_vectors.rs`;
  - full-model parity on the English root, `tests/laya_parity.rs` (ids 14/14, max |dp| 3.841e-6).
- **What re-arms it:** the first artifact `just laya-verify` accepts. That needs the calibration spike
  (`.planning/todos/pending/spike-laya-calibration-slice-and-temperature-cap.md`) and then a declared
  run that passes the gate unchanged.
- **Alternatives that need a user decision:** (1) amend `demo.fail_closed_rule` to allow a local,
  never-uploaded serve of a vector; or (2) add a non-production evidence variant. Option 2 is a
  decide-apr-v1 / laya-finetune-gate-v1 schema change, and D-17 makes it costly.

### D-ITEM-08-09-B: the 1e-5 re-score bar is fragile on real checkpoints (queued, not acted on)

- status: open
- **Found during:** the 08-09 tracer halt and debug session `.planning/debug/resolved/laya-rescore-drift.md`.
- **Facts:**
  - `pack_rescore_probs_abs` 1e-5 sits at the fp32 noise floor of real Laya checkpoints. Against a
    float64 reference, torch's own fp32 is up to 3.7e-5 off on fixed_epochs (13 rows).
  - early_stopping and the base re-score at 6.7e-6 / 6.8e-6, about 1.5x headroom. One 1-ULP codegen
    change (`__sincosf_stret` fusion lost in 71e2306e5) once consumed that headroom.
  - x86_64 (Lambda) has never been measured.
- **Related headroom seen in 08-09:** the full-model ladder's `final` block measured 9.155e-5
  against `final_norm_abs` 1e-4 (1.09x). Spike 025 measured 3.05e-5 there before the RoPE fix.
  Every other ladder block and the 14-row probabilities keep more than 2.5x headroom.
- **Owner:** the calibration spike todo (section "Added 2026-09-26: is the 1e-5 re-score bar above fp32
  noise"). User decision option A keeps the bar, and nothing is changed here.

### D-ITEM-08-09-C: `demo.fail_closed_rule` prose still says both vectors fail on the ece_post clause

- status: open
- **What:** laya-finetune-gate-v1 `demo.fail_closed_rule` says pack and verify MUST refuse each vector
  "with the gate recomputed in Rust ... reproducing FAIL on the ece_post clause". Under option A,
  fixed_epochs refuses earlier, with RescoreDrift (still fail-closed, nothing written).
  FALSIFY-LAYA-GATE-010 (1.3.0) states the per-vector refusal and is the bound test.
- **Why not changed here:** the plan forbids moving any `demo` value, and the user's option A kept
  contract text other than GATE-010 unchanged. Whether to align the prose with GATE-010 is a one-line
  user call.

### D-ITEM-08-09-D: D-ITEM-08-01-A re-measured; the strict-binding PASS line is still unreachable

- status: open
- **Symptom:** unchanged: `VACUOUS: strict-test-binding gate was SKIPPED`, because
  `contracts/spectral-indices-v1.yaml` has no `kani_harnesses:`.
- **Measured around it** (temporary lifted copy, deleted after each run):
  - Task 2: 658 references resolved (644 at 08-06, plus this plan's 14 verify bindings).
  - Task 3: 661 (plus FALSIFY-LAYA-GATE-010 and the two FALSIFY-LAYA-PARITY-001/-003 legs). A
    mutated GATE-010 binding (`demo_vectors_are_refused_fail_closed_mutant_zz`) was flagged
    dangling in the same kind of run, so the resolver discriminates.
  - No Phase 8 contract dangles. The only FAIL lines are the two pre-existing contracts
    (chronos-bolt-parity-v1, setfit-encoder-conformance-v1).
- **Why not fixed here:** same reason as D-ITEM-08-01-A.

## From plan 08-10

### D-ITEM-08-10-A: one decide model per workspace under the shared-crates-root deploy

- status: open
- **What:** cargo-pmcp resolves `<root>/<server>-lambda` first, so the deploy from `crates` works only
  with server name `aprender-mcp-decide`, the package stem. `laya-deploy-config` and `laya-deploy`
  refuse any other name. A second decide model, such as a second task, cannot deploy from this
  workspace without revisiting the 08-10 decision.
- **Also shared:** `crates/.pmcp/` and `crates/deploy/` belong to the setfit training server.
  `_laya-crates-root-swap` backs them up and restores them byte-identically. The selftest proves
  this on success, forced failure, SIGTERM and an absent root. A SIGKILL mid-deploy cannot run
  the trap, so it leaves the backup at `models/decide/swap-backup/crates`, and the next swap
  refuses until a human restores from it.
- **Owner:** D-ITEM-08-10-B removes the constraint.

### D-ITEM-08-10-B: fix the resolver upstream in cargo-pmcp (recommended future SDK work, not done)

- status: open
- **What:** in `find_lambda_package_dir`, when `project_root` is itself a package whose name ends in
  `-lambda` and has a `bootstrap` bin, return it before `find_workspace_lambda_package_dir`. This
  makes `--manifest-path crates/<pkg>-lambda` correct for every server. It also protects the
  chronos and setfit deploys from the same class of bug, and it removes the config swap.
- **Why not done here:** the user chose shared-crates-root. The SDK checkout is an external repo on
  an unrelated branch with uncommitted work, so this plan only read it (`git archive` into a
  scratch dir).

### D-ITEM-08-10-C: the live halves of the deploy recipes have never run against AWS

- status: open
- **Kind:** unrun-verify. The live deploy is deferred by option 3.
- **Proven offline:** `bash -n` passes on every recipe body. The refusals are proven on the
  synthetic artifact: placeholder, sha-pin, resolver-proof, deploy-eligibility and
  upload-eligibility. An aws recorder with a positive control counted 0 AWS calls. The
  `laya-deploy-verify` DRY_RUN plan was built from the artifact, also with 0 AWS calls.
  `laya-verify` precedes the first AWS call in `laya-deploy` and `laya-upload`.
- **Assumptions only a live run can check:**
  - The pmcp.run function name equals the server id. `pmcp-train-grant` relies on the same.
  - cargo-pmcp writes `[deployment] endpoint` to `crates/.pmcp/deployment.toml`, which the swap
    copies to `models/decide/deploy-<server>.state/`.
  - A GET on that `/mcp` endpoint reaches the bootstrap's health branch, which may not hold with
    auth on.
  - The `Compiling` lines of `cargo lambda build` land in the redirected deploy log.
  - `LoggingConfig.LogGroup` names the function's log group.
- **Owner:** the first post-spike deploy. Plan 08-11 records HOLD.

### D-ITEM-08-10-D: `auth=on` sets only `[auth] enabled`

- status: open
- **What:** `laya-deploy-config <apr> on` sets `enabled = true` and leaves `provider = "none"` from
  the template. The provider that pairs with an authenticated pmcp.run function is plan 08-11's
  auth decision. It is not assumed here.


## From plan 08-11

### D-ITEM-08-11-A: the D-18 live deploy and the live `accepted_region_cold` falsification are DEFERRED (option 3; HOLD decided by the user, 2026-09-26)

- status: open
- **Deferred:** two things. (1) The D-18 live deploy of a trained decision model on pmcp.run. (2) The
  live falsification of decide-tool-boundary-v1 `accepted_region_cold`, FALSIFY-DECIDE-TOOL-009, which
  stays `LIVE-PENDING`. The user decided HOLD at the 08-11 go/no-go checkpoint, choosing the
  `hold-no-aws` option. No AWS or pmcp.run call of any kind was made, read-only calls included.
  The record is `08-DEPLOY-EVIDENCE.json` (`outcome: hold`, `decided_by: human`, `readiness: null`).
- **Why:** the D-19 demo failed laya-finetune-gate-v1 under both declared recipes. Both runs are
  fail-closed vectors (`d0f4e40d…` fixed_epochs, `3d4b91da…` early_stopping), and 08-09 refuses both.
  No artifact passes `just laya-verify`, so 08-10's AWS-writing recipes would refuse anything that
  exists in this phase.
- **Next direction (user, 2026-09-26):** pursue a deployable model next to confirm the direction:
  first the calibration spike, then a declared gate run. The procedure below is the path once a
  declared run passes.
- **Comes first:** `.planning/todos/pending/spike-laya-calibration-slice-and-temperature-cap.md`. Whatever
  it recommends is declared in laya-finetune-gate-v1 BEFORE the next gate run is read (D-07).
- **Re-open condition:** a DECLARED run passes laya-finetune-gate-v1 unchanged (gate_max_ece 0.10),
  and `just laya-verify` prints `deploy_eligible true` on its exact `.apr`. The deploy is then a NEW
  plan, not a re-run of 08-11.
- **Decisions still open for that run:**
  - **Auth posture.** Auth on was recommended for a 10 GB function (RESEARCH §Security Domain V2:
    an open 10 GB function is a cost-amplification vector). `laya-deploy-config` takes `auth` as a
    required argument, and the provider that pairs with `auth=on` is still undecided (D-ITEM-08-10-D).
  - **Account and memory facts, still unmeasured.** hold-no-aws recorded no readiness facts.
    - RESEARCH A10: the `ze-kasher-dev` profile resolves, and pmcp.run functions are visible in it.
    - RESEARCH A2: pmcp.run accepts 10,240 MB.
    - The account has not been checked for a function whose name contains `decide` or `laya`.
    - Measure all three with read-only calls (`aws sts get-caller-identity`,
      `aws lambda list-functions`, `aws lambda get-account-settings`) before that plan's first write.
  - **The live-side assumptions of D-ITEM-08-10-C** (function name equals server id,
    `[deployment] endpoint` location, GET health with auth on, compile-log capture, log-group
    naming).
- **Procedure it will follow, in order, with 08-10's recipes:**
  1. `just laya-weights-bucket`
  2. `just laya-upload <apr> <run> <data> <base> <server>`. This is gated by `laya-verify` before
     any AWS call.
  3. `just laya-deploy-config <apr> <auth>`, with the auth posture decided above.
  4. `just laya-deploy <apr> <run> <data> <base> <server>`. The recipe runs these steps:
     1. eligibility through `laya-verify`;
     2. the resolver proof, pinned to the installed cargo-pmcp;
     3. the deterministic compile-log identity check;
     4. the scoped grant (`laya-grant`);
     5. the GET health body naming the package;
     6. the live identity probe.
     On an identity failure, it runs containment (`laya-teardown`).
  5. `just laya-deploy-verify <apr> <server>`. The cold-sample rules:
     - bump the config before each cold sample;
     - make the maximal `tools/call` the first POST;
     - take at least 2 CONCENTRATED and 2 DISTRIBUTED samples;
     - prove each one cold by CloudWatch `performed_load=true`;
     - require every sample to be under 30000 ms.
  6. On any breach, record the samples unaltered and stop at a blocking-human choice between
     three responses:
     - lower the contract-owned budget through a gap plan (D-10);
     - defer while serving;
     - defer and contain.
- **Full pre-revision text:** `git show 59d9ed07f:.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-11-PLAN.md`
  (Tasks 2-4: deploy with identity asserted, both cold shapes on every sample, the exceeded-region
  response, the outcome record).
