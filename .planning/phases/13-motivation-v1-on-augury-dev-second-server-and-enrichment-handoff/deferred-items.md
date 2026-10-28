# Phase 13 deferred items

## From plan 13-04

- **laya-gates-selftest does not restore the server id's eligibility logs.** The rows deploy-refusals,
  upload-eligibility and sha256-helper call `just laya-deploy` and `just laya-upload` directly with
  `SRV` (default aprender-mcp-decide-antisemite). That rewrites `models/decide/eligibility-<id>.log` and
  `eligibility-upload-<id>.log`, and the sweep's cleanup does not back them up. laya-deploy-selftest
  has restored them since plan 13-01; the gates selftest never has. This predates plan 13-04.
  - Observed in plan 13-04: the Task 2 filtered sweep replaced the antisemite id's two logs. Both the
    old and the new bytes were tiny-fixture `REFUSED SyntheticNotDeployable` logs; only the build time
    and the fixture path changed (4 lines each).
  - The executor restored the plan-start bytes from a pre-plan backup, and `_laya-state-snapshot`
    then matched the plan-start snapshot.
  - The motivation id's ze-kasher-dev record (the 13-11 manifest) was never touched, and it verified
    after every run.
  - Fix for a later plan: add the two eligibility paths to the gates selftest's backup/restore list,
    exporting each run's output first, as 13-01 did for the deploy selftest.

## From plan 13-06

- **The same gates-selftest defect reproduced.** Task 2's filtered sweep (row deploy-refusals) rewrote
  `models/decide/eligibility-aprender-mcp-decide-antisemite.log` again (tiny-fixture refusal log; only the
  build time and fixture path change). The executor restored the plan-start bytes from a pre-plan backup
  (`cmp` equal), and `_laya-state-snapshot` then matched the plan-start snapshot. Because the plan's verify
  takes its first snapshot BEFORE the sweep, its final cmp tripped on this file; the deploy selftest was
  re-bracketed alone (snapshot before and after it, equal). The fix named above is still open.
