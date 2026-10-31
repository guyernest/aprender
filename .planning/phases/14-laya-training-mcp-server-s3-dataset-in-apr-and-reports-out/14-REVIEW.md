---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
reviewed: 2026-10-30T00:00:00Z
depth: standard
scope: incremental re-review of the gap-closure round 14-16..14-22 (diff base 2ead5836f)
files_reviewed: 16
files_reviewed_list:
  - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
  - crates/aprender-mcp-laya-train-lambda/src/objects.rs
  - crates/aprender-mcp-laya-train-lambda/src/sfn.rs
  - crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs
  - crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
  - crates/aprender-mcp-laya-train/src/finalize.rs
  - crates/aprender-mcp-laya-train/src/layout.rs
  - crates/aprender-mcp-laya-train/src/lib.rs
  - crates/aprender-mcp-laya-train/src/redact.rs
  - scripts/laya_deploy/laya_failure_surface.py
  - scripts/laya_deploy/laya_drift.py
  - scripts/laya_deploy/laya_trainer_ops.py
  - scripts/laya_deploy/laya_trainer_stack_check.py
  - scripts/laya_gates.tsv
  - justfile
  - deploy-extensions/README.md
findings:
  critical: 0
  warning: 3
  info: 9
  total: 12
status: issues_found
---

# Phase 14: Code Review Report (incremental, gap round 14-16..14-22)

**Reviewed:** 2026-10-30
**Depth:** standard
**Files Reviewed:** 16
**Status:** issues_found

## Summary

This review replaces the 2026-10-29 report. It covers the changes since `2ead5836f`: outcome-first classification
(14-17), total finalization over listed-but-absent objects (14-17), caller-safe text (14-18), finalizer provenance
(14-19), the failure-surface enumeration and its checker (14-16, 14-19), the drift harness (14-20, 14-21), and the
gate rows and recipes that wire them in.

All four prior warnings and IN-01 are fixed as the previous report asked. Each fix is in the code, not only in the
documents, and has a test that drives both delivery paths or a planted failure. I found no injection, path-traversal
or authorization defect in the new code. The generated Graviton script quotes every value it embeds and checks each
one against a strict grammar first. The mutation runner restores files byte for byte and checks that it did. The
failure-surface binding compares both directions.

What remains are three gaps, each a case the new protection does not reach:

- **WR-01:** in `finalize.rs`, job-written text is clipped before it is redacted, which defeats the truncation rule
  `caller_safe` documents.
- **WR-02:** the finalizer provenance record attests whatever binary sits at a hard-coded `target/` path, not the
  binary the build just produced.
- **WR-03:** the request function now holds the WR-03 redaction boundary, but it is deployed with no build record at
  all. The WR-04 fix covered only one of the two Lambda binaries.

### Prior findings: disposition with evidence

| Prior | Status | Evidence |
|-------|--------|----------|
| WR-01 (classification unreachable) | **resolved** | `finalize.rs:533-546`: the `stage_error` early return is gone. The stage error is used only when `outcome` is `None`. A parsed outcome now decides every class: the refusal arm sets `prefilter_disagreement` true whatever the delivery path, and crash, timeout, device, missing gate report and missing pack record append `; stage error: ...` through `Builder::with_stage`. `CATCH_FED_CASES` together with `deliveries()` (`finalize.rs` tests) decide each of the five classes twice, once through `fixtures::through_catch` (the real `redact::stage_error` shape) and once with `stage_error: None`, and assert the same phase, `is_error` and D-08 flag both times. `KNOWN_DEFECTS` is empty (`finalizer.rs:3923`), and the shrink-only ratchet asserts that. |
| WR-02 (wedge on listed-but-absent) | **resolved** | `finalizer.rs:573-604` (`drop_absent_entries`) removes every manifest entry the prefix listing does not hold before the decision, so the decision, the payload, `manifest.json` and the links are built only from objects that exist, and the dropped paths go into a bounded note (`finalize.rs` `dropped_note`). `publish` restricts the tag targets to a listing taken after `verdict.json`/`manifest.json` were written (`finalizer.rs:950-964`), so an adopted manifest that names a vanished object no longer fails `GetObjectTagging` forever. `list` is paginated (`objects.rs:182-205`). |
| WR-03 (raw AWS text to callers) | **resolved, residual in WR-01 below** | Three boundaries are in place. (1) `StatusPayload::to_value` (`layout.rs:567-586`) passes `error`, `verdict.pack.line` and `notes` through `caller_safe`, and the stored result, `verdict.json`, `tasks/result` and `train_status` all serialize there (`finalize.rs:178-181`, `finalizer.rs:644`, `lib.rs:676,705`). (2) The Catch `{Error, Cause}` is *reduced*, not forwarded: `redact::stage_error` keeps an identifier name plus whitelisted reason keys, and `finalizer.rs:269-278` logs the full text server-side. (3) `RedactingBackend` wraps the DynamoDB backend in `bootstrap.rs`, and `start_error_label` (`sfn.rs:55-83`) turns a failed `StartExecution` into a bare code. `describe` and `stop` errors are log-only (`lib.rs:579-581`, `sfn.rs:487-499`). |
| WR-04 (finalizer asset provenance) | **resolved for the finalizer, residuals in WR-02 and WR-03** | `laya-trainer-synth` writes `bootstrap.build.json` (`justfile:1417`, `laya_trainer_ops.py:792-832`). `cmd_infra` refuses `diff`/`deploy` on a missing sidecar, a sha mismatch, a dirty build closure, a non-ancestor commit, a closure changed since the build, or a closure dirty now (`laya_trainer_ops.py:738-763,937-947`), and does this before any `cdk` call. The closure comes from `cargo tree`. I re-derived it with `-e normal` and with `-e normal,build` for `aarch64-unknown-linux-gnu`: both give the same 12 local paths, so excluding build edges loses nothing today. |
| IN-01 (inventories expire on a kept run) | **resolved** | `SELF_LISTING` now marks retention-tag targets (`finalizer.rs:65-75`). `publish` adds all five names and keeps the ones that exist (`finalizer.rs:962-964`). |
| IN-02 (immutable tag blocks a re-build) | **open** | `cmd_image` still starts the CodeBuild build without first checking `ecr describe-images --image-ids imageTag=<HEAD>` (`laya_trainer_ops.py:556-592`). Carried as IN-04 below. |
| IN-03 (gate-contract skew unobserved) | **open** | `Builder::manifest` still records `prefilter_gate` and `train_gate` and never compares them (`finalize.rs:338-349`). Carried as IN-05 below. |

## Warnings

### WR-01: job-written text is clipped to 1024 characters BEFORE redaction, so a partial account id or access key can survive

**File:** `crates/aprender-mcp-laya-train/src/finalize.rs:273-275` (`clip`), used at `:381` (`payload.error = error.map(|e| clip(&e))`) and `:443` (`pack_info`, `.map(|l| clip(&l))`); the redaction runs later, in `layout.rs:567-586`
**Issue:** `caller_safe` redacts first and truncates second. Its truncation also refuses to leave a partial span that
would be sensitive on its own (`redact.rs:244-264`; the module doc calls this the property that makes it idempotent
and safe). `finalize.rs` reverses that order. `Builder::decide` and `pack_info` cut free text to `MAX_ERROR_CHARS` =
1024 characters with a plain `chars().take(1024)`, and only afterwards does `to_value` redact. A 12-digit account id
or a 20-character `AKIA…` key that straddles character 1024 is cut to its first 1-11 digits (or `AKIA` plus 15
characters). The rule cannot recognise that fragment: `has_account_digits` needs a run of exactly 12, and
`has_access_key` needs 16 tail characters. So the fragment reaches `error` / `verdict.pack.line` in the stored payload,
`verdict.json`, `tasks/result` and `train_status`. Eleven of twelve digits is the account id up to ten guesses.

The text clipped here is not limited to fixed sentences. It includes job-written lines:

- `outcome.refused_line`, the first `REFUSED ` line of the trainer log, with no length bound (`run_step.py:198-207`), embedded in `format!("the trainer refused ...: {line}")`;
- `packed_line` / `verify_line`, cut by the container to 4000 characters (`run_step.py:1443-1444`, also before any redaction).

A pack or verify `REFUSED` line that quotes an S3 URI (the bucket is `aprender-laya-train-<env>-<account>`) or an ECR
reference near offset 1024 is the realistic trigger. The redact test table never puts an identifier at the
1024-character clip boundary, so the tests do not catch this.

**Fix:** let the redaction see the whole span before anything is cut. Replace `clip` with the redaction-aware bound so
there is one truncation rule:
```rust
fn clip(text: &str) -> String {
    // Redact first, then truncate (the partial-span rule lives in caller_safe).
    crate::redact::caller_safe(text, MAX_ERROR_CHARS)
}
```
Because `caller_safe` is idempotent, the second pass in `to_value` changes nothing. Add a must-redact row that places
`987654321098` and `AKIAIOSFODNN7EXAMPLE` so they straddle character 1024 of a `refused_line`, and assert that no
digit run of 6 or more and no `AKIA` prefix survives in `finalize_run(..).payload.to_value()`.

### WR-02: the provenance record attests the binary at a hard-coded `target/` path, not the binary the build produced

**File:** `justfile:1407-1417` (`cargo zigbuild ...`, then `BIN="target/{{target}}/release/aprender-laya-train-finalizer"`, `cp "$BIN"`, `finalizer-provenance write`); `scripts/laya_deploy/laya_trainer_ops.py:792-809` (`build_sidecar`)
**Issue:** `finalizer-provenance write` hashes whatever file is at `$ASSET/bootstrap` and stamps it with
`git rev-parse HEAD` and a clean-closure flag. The file it hashes is copied from a fixed `target/...` path. Cargo writes
to `$CARGO_TARGET_DIR` (or a `build.target-dir` in a host-local `.cargo/config.toml`) when either is set. In that case
`cargo zigbuild` puts the fresh binary somewhere else, and `cp` copies whatever older binary is still under `./target/`.
The sidecar then records that stale binary as built from HEAD with `closure_clean: true`. `cmd_infra` accepts it,
because the sha matches the sidecar, the commit is HEAD and the closure is clean. That is the WR-04 failure this round
set out to close: a deploy silently ships an unreviewed or stale finalizer. The README states "this binary is the one
the synth produced" (`deploy-extensions/README.md`, "What it proves"), and this case makes that statement false. The
repo already knows the safe pattern. `laya_drift.py:binary_identity` takes the executable path from cargo's own
`compiler-artifact` message, and the embed-probe recipe derives `TD` from `cargo metadata` (`justfile:8972-8974`).
**Fix:** take the binary path from the build itself, for example:
```bash
TD=$(cargo metadata --no-deps --format-version 1 | python3 -I -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
BIN="$TD/{{target}}/release/aprender-laya-train-finalizer"
```
Better still, have `finalizer-provenance write` run (or parse) `cargo zigbuild ... --message-format=json` and hash the
`executable` it names. Also record cargo's `fresh` flag, so that a "built" binary which was really a cache hit from
another tree is visible.

### WR-03: the request function (now the caller-facing redaction boundary) is deployed with no build attestation

**File:** `scripts/laya_deploy/laya_trainer_ops.py:1557-1625` (`cmd_deploy`)
**Issue:** WR-04 was fixed for the finalizer only. Since 14-18, the request function's binary holds most of the WR-03
protection: `RedactingBackend` (`bootstrap.rs`), `start_error_label` (`sfn.rs`), and the `caller_safe` calls in `submit`
and `status` (`lib.rs:584-597,659-667`). `cmd_deploy` builds that binary through `cargo pmcp deploy` from whatever the
working tree holds. It never checks that the closure is clean or committed, and it records nothing about what it
built. `deploy-<env>.json` holds only the endpoint and a timestamp. The checks it does make (the package name appears
in the `Compiling` lines, the endpoint names the server) prove which crate was built, not from what source. A deploy
from a tree where `redact.rs` or `lib.rs` is modified or reverted is indistinguishable from a reviewed one. That is the
same class of gap WR-04 described, now on the binary that serves callers directly. This round's own redeploy (14-22)
went through this path.
**Fix:** reuse the 14-19 machinery. Derive the closure with `cargo tree -p aprender-mcp-laya-train-lambda --bin
bootstrap ...` (the same `parse_closure`). Refuse a live `deploy` when `git_dirty` over that closure is non-empty, and
write `commit`, `closure`, `closure_clean` and the deployed zip's sha256 (from the cargo-pmcp build output) into
`deploy-<env>.json`. Then a later `verify` can say what is actually serving callers.

## Info

### IN-01: stage-specific wording is wrong on two catch-fed paths

**File:** `crates/aprender-mcp-laya-train/src/finalize.rs:118-121` and `:450-457`
**Issue:**
- `NOTE_STAGE_ERROR_OVERRIDDEN` always says "the train job exited non-zero". The same note is attached when the stage
  error came from the PackVerify Catch (`deploy-extensions/lib/laya-trainer-stack.ts:509`) and the pack records decided
  `completed`.
- When the Train job exits 1 after recording a `pass` (an upload failed), the machine skips PackVerify. The caller then
  reads "pack-outcome.json is missing: the pack stage left no record". The pack stage never ran, so that sentence
  points at the wrong stage.

**Fix:** make the note neutral ("a stage exited non-zero although ..."). In `decide_packed`, when `stage_error` is
`Some` and `pack_outcome` is `None`, say "the pack stage did not run: the training stage failed after recording its
outcome".

### IN-02: payloads stored before the 14-18 redeploy are re-served unredacted on two of the three doors

**File:** `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs:626-641` (`Winner::from_result` keeps the stored text) and `:870-873` (verdict.json is that text byte for byte)
**Issue:** `train_status` re-parses a stored result and re-serializes it through `to_value` (`lib.rs:666-676`), so it
redacts old records. pmcp's own `tasks/result` serves the stored text verbatim, and a re-finalization copies it
verbatim into `verdict.json`. A record written by the pre-14-18 finalizer with a raw SFN Cause keeps that text on
those two doors until its TTL expires. This is bounded by the task TTL, so the exposure ends on its own. It is
recorded so the "every door" claim in `layout.rs:556-565` is read with that caveat.

**Fix:** none needed if the pre-redeploy records have expired. Otherwise re-serialize in `Winner::from_result`
(`payload.to_value().to_string()`) when `verdict.json` is written.

### IN-03: known blind spots of the redaction blacklist

**File:** `crates/aprender-mcp-laya-train/src/redact.rs:158-208`
**Issue:** The rule does not redact these forms, as written and as tested:
- the console's hyphenated account form `9876-5432-1098`;
- a 12-digit run directly preceded or followed by an ASCII letter (`acct987654321098`);
- IAM unique-id prefixes other than `AKIA`/`ASIA` (`AROA…`, `AIDA…`). These are not secrets, but they identify principals.

`stage_error(Some("AKIA…"), None)` also swallows the separator ("[redacted] no cause recorded"). None of these forms
appears in an AWS SDK message I know of. They are listed so the doc's "no account id ... reaches a caller" is read as
"no account id in a known shape".

**Fix:** add `\d{4}-\d{4}-\d{4}` and the `A[RI]DA`/`AGPA`/`ANPA` prefixes to `sensitive`, with case-table rows.
Optional.

### IN-04: (carried, prior IN-02) an immutable commit tag still makes a partial image build for a commit impossible to repeat

**File:** `scripts/laya_deploy/laya_trainer_ops.py:556-592`; `deploy-extensions/lib/laya-trainer-images-stack.ts:72`
**Issue:** Unchanged since the prior review.
**Fix:** As before, check `describe-images --image-ids imageTag=<HEAD>` before `start-build`. If the tag exists,
record that digest and do not rebuild.

### IN-05: (carried, prior IN-03) pre-filter and trainer gate contracts can still skew with no signal

**File:** `crates/aprender-mcp-laya-train/src/finalize.rs:338-349`
**Issue:** Unchanged. `prefilter_gate` and `train_gate` are both recorded and never compared. With WR-01 (prior)
fixed, a trainer refusal does now set `prefilter_disagreement`. A skew that changes no refusal, such as the
calibration constants, is still invisible.
**Fix:** As before. When both hashes are present and differ, add a fixed `notes` entry (the 14-18 notes field is the
natural place).

### IN-06: (known, agreed) `_laya-crates-root-swap` is labelled as a decide-side recipe

**File:** `justfile:2286-2297`
**Issue:** The trainer deploy uses it, but it refuses with "decide config ... does not exist" and backs up to
`models/decide/swap-backup/`. The label is cosmetic and the behaviour is correct.

### IN-07: (known, agreed) no pmcp.run login precheck before the grant revoke

**File:** `scripts/laya_deploy/laya_trainer_ops.py:1500-1548,1578-1580`
**Issue:** `cmd_deploy` revokes the grant (`revoke_grant`) before `cargo pmcp deploy`. An expired pmcp.run login makes
the deploy fail after the revoke, which leaves the function with no grant until a re-run.
**Fix:** probe the pmcp.run session (or `cargo pmcp` auth status) before `revoke_grant`.

### IN-08: (known, accepted AR-7) the generated drift script has no total task cap and no stop-task on timeout

**File:** `scripts/laya_deploy/laya_drift.py:992-996` (the template's `TIMEOUT ... it is still running` branch) and `:1022-1032` (the repeat loop)
**Issue:** One invocation runs up to 5 repeats per leg with no total cap, and the 45-minute poll timeout exits and
leaves the Fargate task running. This was accepted as AR-7 and I agree with the acceptance. Every write is still
expire-tagged and read back before a task starts.

### IN-09: (known, W-5) real account ids in the committed justfile target table

**Issue:** This predates the round and is recorded in 14-SECURITY.md W-5. It is mentioned only so this report does not
appear to have missed it. The new files in scope carry only the reserved fixture ids `111122223333` and
`999988887777` (`laya_drift.py`), and none in `failure-surface-v1.json` or the README. I checked with a 12-digit scan.

---

_Reviewed: 2026-10-30_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
