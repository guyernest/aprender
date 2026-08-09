---
status: partial
phase: 02-deterministic-pair-and-data-protocol
source: [02-VERIFICATION.md]
started: 2026-08-09T08:33:23Z
updated: 2026-08-09T08:33:23Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. Accept or reject the setfit row-byte deviation (FINDING-D1)

Plan 02-06's `must_haves.truths[2]` asserts the relocated loader stays byte-identical to
the D-06 baseline. That is now literally false for the **setfit** profile: all 346 merged
rows carry `source_split: "compatibility_test"` where the baseline wrote `validation` /
`test`. It remains true for the canonical profile.

The change was forced, not incidental: D-19 requires the merged split to be
`Split<CompatibilityTest>`, and plan 02-03's Gate 2 requires every row's `source_split` to
equal the role being built. It was disclosed via `schema_version` 1 -> 2, a
`profiles.setfit.row_source_split` field, and a pinning test.

The verifier classified this UNCERTAIN rather than FAILED, on the grounds that reverting it
would break D-19 and therefore ROADMAP success criterion 5 — so routing it to gap closure
would be actively wrong.

expected: A human accepts the deviation (ready-to-paste override YAML is in
02-VERIFICATION.md), or rejects it and specifies how D-19 should otherwise be satisfied.
result: [pending]

### 2. Approve the crates.io publish cascade

`apr-cli` now depends on `aprender-contrastive-data`, which is not on crates.io. Every form
of `cargo package -p apr-cli` therefore fails, including `--no-verify` (which still performs
the manifest resolution that rewrites the path dep into a registry dep). The crate-only form
is green at 59 files.

This is the only outstanding item behind the known-red pre-release Gate 5. Clearing it needs
`aprender-contrastive-data` published **before** `apr-cli`. CLAUDE.md forbids an agent
self-serving the publish cascade, so no executor attempted it.

expected: A human approves and runs the publish cascade in that order, or defers it and
accepts Gate 5 staying red until they do.
result: [pending]

### 3. Resolve the untested FALSIFY-CPP-007 prediction (FINDING-W2)

`FALSIFY-CPP-007` in `contracts/contrastive-pair-protocol-v1.yaml` predicts N=512 with
`C(512,2) = 130816`. The maximum K actually exercised anywhere in the suite is 128, and the
literal `130816` appears nowhere in the tree. The prediction is currently unfalsifiable —
the same "a gate that cannot fail for the reason it claims" class this phase repeatedly
surfaced elsewhere in the repo.

Two honest resolutions: add a K=512 case so the prediction is exercised, or amend the
prediction to the layout that is actually tested. Left for a human because choosing between
them is a scope call, not a mechanical fix.

expected: Either a K=512 case exists and passes, or FALSIFY-CPP-007 states a bound the suite
actually tests.
result: [pending]

## Summary

total: 3
passed: 0
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps

None. The verifier found no gaps, no blockers, and no debt markers anywhere in the phase
diff, and scored 5/5 ROADMAP success criteria VERIFIED (31/32 must-haves). Phase 3 is not
blocked by any of the three items above.

FINDING-W1 (a stale module-doc comment claiming `run_pairs` was still a placeholder) was a
plain defect rather than a decision and was fixed directly in `f3ee9c0d2`.
