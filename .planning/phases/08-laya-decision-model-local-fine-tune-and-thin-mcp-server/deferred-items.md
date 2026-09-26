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
