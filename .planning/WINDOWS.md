---
schema_version: 1
open_count: 0
waived_count: 2
fixed_count: 0
total_count: 2
last_updated: 2026-08-17T04:44:14.174Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 05 | deviation | crates/aprender-core/src/calibration_tests.rs |  | 05-04 T2: the plan's named RED mutation (bin by predicted-class probability) is a no-op since pred IS the argmax; substituted true-label-confidence and unweighted-bins mutations | waived | Not a defect: the plan's named RED mutation is provably a no-op (predicted class IS the argmax), and 05-04 shipped two mutations that DO discriminate, with recorded values. Documented in 05-04-SUMMARY.md Deviations. Nothing is left open in the code. | 2026-08-17T04:43:03.419Z | 2026-08-17T04:44:08.106Z |
| 2 | 05 | deviation | crates/aprender-core/src/stats/tests_claims_stats.rs |  | 05-04 T3: the plan's RNG-guard criterion lists 'sample' as an RNG symbol; it is the statistical noun and the pre-existing f32 API's parameter name, so the guard bans real RNG symbols instead | waived | Not a defect: banning the token 'sample' would ban the pre-existing f32 ttest_1samp(sample: &[f32]) API and catch no RNG. The shipped guard bans rand::/thread_rng/SeedableRng/StdRng/gen_range/bootstrap/resample/shuffle/random( and passes. Documented in 05-04-SUMMARY.md Deviations. | 2026-08-17T04:43:13.479Z | 2026-08-17T04:44:14.174Z |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "05",
    "file": "crates/aprender-core/src/calibration_tests.rs",
    "line": null,
    "description": "05-04 T2: the plan's named RED mutation (bin by predicted-class probability) is a no-op since pred IS the argmax; substituted true-label-confidence and unweighted-bins mutations",
    "status": "waived",
    "reason": "Not a defect: the plan's named RED mutation is provably a no-op (predicted class IS the argmax), and 05-04 shipped two mutations that DO discriminate, with recorded values. Documented in 05-04-SUMMARY.md Deviations. Nothing is left open in the code.",
    "recorded_at": "2026-08-17T04:43:03.419Z",
    "resolved_at": "2026-08-17T04:44:08.106Z"
  },
  {
    "id": 2,
    "kind": "deviation",
    "phase": "05",
    "file": "crates/aprender-core/src/stats/tests_claims_stats.rs",
    "line": null,
    "description": "05-04 T3: the plan's RNG-guard criterion lists 'sample' as an RNG symbol; it is the statistical noun and the pre-existing f32 API's parameter name, so the guard bans real RNG symbols instead",
    "status": "waived",
    "reason": "Not a defect: banning the token 'sample' would ban the pre-existing f32 ttest_1samp(sample: &[f32]) API and catch no RNG. The shipped guard bans rand::/thread_rng/SeedableRng/StdRng/gen_range/bootstrap/resample/shuffle/random( and passes. Documented in 05-04-SUMMARY.md Deviations.",
    "recorded_at": "2026-08-17T04:43:13.479Z",
    "resolved_at": "2026-08-17T04:44:14.174Z"
  }
]
````
