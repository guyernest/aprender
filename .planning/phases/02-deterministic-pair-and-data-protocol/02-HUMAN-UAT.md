---
status: partial
phase: 02-deterministic-pair-and-data-protocol
source: [02-VERIFICATION.md]
started: 2026-08-09T08:33:23Z
updated: 2026-08-09T09:05:00Z
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

### 4. Triage three code-review blockers (02-REVIEW.md)

The independent code review found 3 blockers and 14 warnings across 34 files. The library core
held up under adversarial reading — zero hash-ordered collections, checked capacity arithmetic,
digest-before-parse, genuinely O(K) retained state, correct union-find coalescing. The defects
cluster at the edges.

- **CR-01 path traversal (security).** `data_contrastive.rs:256-262` joins each untrusted
  `attestation.splits` role onto `--data` *before* `preflight` validates the role set.
  `Path::join` with an absolute component replaces the base, so a crafted `"role": "/etc/shadow"`
  reads `/etc/shadow.jsonl`, and `"../../.."` escapes. Gives a file-existence oracle and an
  unbounded read / FIFO hang. The correct pattern already exists in
  `data_tweeteval::verify_prepared_directory`.
- **CR-02 `--force` re-prepare destroys the prior benchmark.** `data_tweeteval.rs` `write_file`
  (line 813) truncates in place, then `remove_all` (line 807) *unlinks* those paths when
  `verify_prepared_directory` fails — so a "rollback" destroys files that pre-existed the run.
  This phase built a correct `atomic_write_with` next door and did not use it here.
- **CR-03 the new tier2/tier3 gates cannot fail under GNU Make 4.x.** VERIFIED by the
  orchestrator on this host with both make versions installed:

  ```
  SHELL := /bin/bash ; .ONESHELL: ; recipe = { false ; @echo "done" }
  make  3.81 (macOS default)      -> exit=2   failure caught
  gmake 4.4.1 (Linux dev + CI)    -> exit=0   failure SWALLOWED
  ```

  `.ONESHELL:` (Makefile:20) with no `.SHELLFLAGS` override runs the whole recipe in one shell
  with default `-c` and no `-e`, so only the LAST line's status survives. Both tier recipes end
  in `@echo "Tier N: PASSED"`, which always succeeds. Under Make 4.x that disarms every gate in
  them: tier2's `cargo test --lib`, clippy, and the Phase 1 + Phase 2 suites; tier3's
  `cargo test --all`, clippy, all four `check_*.sh` scripts, `contract-validate`, the new
  BLOCKING `contract-audit-phase2`, `setfit-feature-matrix` and `contrastive-data-boundary`.

  **Scope limit, measured:** CI does NOT currently invoke `make tier2`/`tier3` (it runs cargo
  directly), so no CI status check is vacuously green today. The defect bites Linux developers
  running the tiers locally, and would bite CI the moment the tiers are wired in — which D-26's
  own rationale ("a gate outside the tiers is a target that stops being run") encourages.

  This is why plan 02-08's gate-failure proof passed honestly and still did not transfer: it was
  measured standalone on macOS make 3.81, which ignores `.ONESHELL:`. CLAUDE.md rule 4 —
  extending a guard's scope requires re-mutating in the new scope.

  The reviewer's fix is one line, `.SHELLFLAGS := -e -u -o pipefail -c`. NOT applied: it changes
  failure semantics for every recipe in a 1000+ line Makefile (`-u` in particular will trip on
  unset variables, and several targets rely on `|| true`). It needs its own verification pass
  across all targets on BOTH make versions, then re-mutation in the new scope.

expected: A human triages the three blockers — CR-01 and CR-02 as gap-closure code fixes, CR-03
as its own Makefile-hardening task with a full-target re-verification pass.
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps

None. The verifier found no gaps, no blockers, and no debt markers anywhere in the phase
diff, and scored 5/5 ROADMAP success criteria VERIFIED (31/32 must-haves). Phase 3 is not
blocked by any of the three items above.

FINDING-W1 (a stale module-doc comment claiming `run_pairs` was still a placeholder) was a
plain defect rather than a decision and was fixed directly in `f3ee9c0d2`.
