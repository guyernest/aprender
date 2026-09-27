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
