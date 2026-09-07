---
schema_version: 1
open_count: 4
waived_count: 2
fixed_count: 0
total_count: 6
last_updated: 2026-09-07T02:44:08.056Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 05 | deviation | crates/aprender-core/src/calibration_tests.rs |  | 05-04 T2: the plan's named RED mutation (bin by predicted-class probability) is a no-op since pred IS the argmax; substituted true-label-confidence and unweighted-bins mutations | waived | Not a defect: the plan's named RED mutation is provably a no-op (predicted class IS the argmax), and 05-04 shipped two mutations that DO discriminate, with recorded values. Documented in 05-04-SUMMARY.md Deviations. Nothing is left open in the code. | 2026-08-17T04:43:03.419Z | 2026-08-17T04:44:08.106Z |
| 2 | 05 | deviation | crates/aprender-core/src/stats/tests_claims_stats.rs |  | 05-04 T3: the plan's RNG-guard criterion lists 'sample' as an RNG symbol; it is the statistical noun and the pre-existing f32 API's parameter name, so the guard bans real RNG symbols instead | waived | Not a defect: banning the token 'sample' would ban the pre-existing f32 ttest_1samp(sample: &[f32]) API and catch no RNG. The shipped guard bans rand::/thread_rng/SeedableRng/StdRng/gen_range/bootstrap/resample/shuffle/random( and passes. Documented in 05-04-SUMMARY.md Deviations. | 2026-08-17T04:43:13.479Z | 2026-08-17T04:44:14.174Z |
| 3 | 06 | deviation | crates/aprender-forecast/src/prophet.rs |  | objective_at_python_map binds 3 of 7 fixtures: the four spike-003 oracles publish no log_posterior_at_map_unnormalized, so no rung-2 test exists for them (06-03 Deviation 1; stated in contracts/prophet-parity-v1.yaml) | open |  | 2026-09-06T05:38:09.726Z |  |
| 4 | 06 | unrun-verify | contracts/chronos-bolt-parity-v1.yaml |  | quantiles_abs_f32_nonaarch64 = 5.0e-6 is PROVISIONAL and UNMEASURED; the first x86_64 run must record its max\|delta\| and tighten the bar (FALSIFY-CHRONOS-002) | open |  | 2026-09-06T13:46:12.140Z |  |
| 5 | 06 | unrun-verify | .planning/phases/06-native-time-series-forecasting-stack/deferred-items.md |  | D-18 clause 2: no CI leg exercises the embedded-weights build; quantiles_abs_f32_nonaarch64 stays PROVISIONAL 5.0e-6 pending one x86_64 just chronos-gate run (same obligation as REVIEW-06-02 and ledger entry #4) | open |  | 2026-09-06T23:48:33.107Z |  |
| 6 | 06 | deviation | crates/aprender-mcp-forecast/src/lib.rs |  | Plan 06-10 stated 22 existing e2e::refuses_ cases; measured baseline was 21, so post-change is 23 not the plan's >=24 bar. Intent (2 cases added) met; number in plan text was stale. | open |  | 2026-09-07T02:44:08.056Z |  |

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
  },
  {
    "id": 3,
    "kind": "deviation",
    "phase": "06",
    "file": "crates/aprender-forecast/src/prophet.rs",
    "line": null,
    "description": "objective_at_python_map binds 3 of 7 fixtures: the four spike-003 oracles publish no log_posterior_at_map_unnormalized, so no rung-2 test exists for them (06-03 Deviation 1; stated in contracts/prophet-parity-v1.yaml)",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-06T05:38:09.726Z",
    "resolved_at": null
  },
  {
    "id": 4,
    "kind": "unrun-verify",
    "phase": "06",
    "file": "contracts/chronos-bolt-parity-v1.yaml",
    "line": null,
    "description": "quantiles_abs_f32_nonaarch64 = 5.0e-6 is PROVISIONAL and UNMEASURED; the first x86_64 run must record its max|delta| and tighten the bar (FALSIFY-CHRONOS-002)",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-06T13:46:12.140Z",
    "resolved_at": null
  },
  {
    "id": 5,
    "kind": "unrun-verify",
    "phase": "06",
    "file": ".planning/phases/06-native-time-series-forecasting-stack/deferred-items.md",
    "line": null,
    "description": "D-18 clause 2: no CI leg exercises the embedded-weights build; quantiles_abs_f32_nonaarch64 stays PROVISIONAL 5.0e-6 pending one x86_64 just chronos-gate run (same obligation as REVIEW-06-02 and ledger entry #4)",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-06T23:48:33.107Z",
    "resolved_at": null
  },
  {
    "id": 6,
    "kind": "deviation",
    "phase": "06",
    "file": "crates/aprender-mcp-forecast/src/lib.rs",
    "line": null,
    "description": "Plan 06-10 stated 22 existing e2e::refuses_ cases; measured baseline was 21, so post-change is 23 not the plan's >=24 bar. Intent (2 cases added) met; number in plan text was stale.",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-09-07T02:44:08.056Z",
    "resolved_at": null
  }
]
````
