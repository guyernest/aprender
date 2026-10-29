---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 11
subsystem: tooling
tags: [laya, s3, sha256, manifest, fetch, reproduction, gate-row, python-stdlib]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-01 manifest-v1 / gate-report-v1 / pack-outcome-v1 golden fixtures and the laya-train-manifest-v1 layout the fetch reads; 14-04 justfile edits (file-ownership edge)
provides:
  - scripts/laya_deploy/laya_fetch.py, the manifest-verified, byte-exact-or-nothing run fetch (with --selftest)
  - the `laya-fetch` recipe and its registered, mutation-checked gate row `fetch`
  - scripts/laya_deploy/laya_repro_eval.py, the pure D-16 reproduction evaluator (check-declaration, evaluate; with --selftest)
affects: [14-14, 14-15]

plan_head_before: 703f1112bad5d11e01d4fb579f5a7af4d64d47a5
plan_head_after: 6ab0e10e5735e6fbb1cc26c2b7fdb8d395a8b4ac

actuals:
  tokens: 22900
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "one pure dest_for mapping validated over the WHOLE tree before any download; staging dir + one rename"
    - "S3 reads through one swappable Runner; the selftest replaces the real subprocess entry point with a tripwire and asserts it never fires"
    - "an evaluator whose mandatory clause keys are a fixed tuple, never the declaration's own keys, so an empty or partial declaration cannot pass vacuously"

key-files:
  created:
    - scripts/laya_deploy/laya_fetch.py
    - scripts/laya_deploy/laya_repro_eval.py
  modified:
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "verdict.json is the finalizer's copy of the StatusPayload, so the fetch reads `phase` at its top level and the evaluator reads `verdict.waived` (falling back to a top-level `waived`)"
  - "The evaluator compares the seeds the run actually ran with the declaration's own must.seeds (14-14 declares 13/17/23) rather than a hard-coded literal"
  - "deploy_eligible is false for an in-band-gate-fail run (nothing was packed) without that being a defect; the defect rules are what make a run non-retriable"

patterns-established:
  - "A guard that returns None for a JSON `null` file is a vacuous pass: read_json_file now returns a reason for anything that is not an object"

requirements-completed: [D-20, D-18, D-04, D-01, D-16, D-17]

coverage:
  - id: D1
    description: "laya_fetch.py pulls runs/<NAME>/<TASK_ID>/ by its manifest through the closed dest_for mapping, verifying every entry's size and sha256 in staging before one rename; refuses dup, conflict, layout, path, manifest, dest, name, task, bucket, profile, incomplete, size, sha256 with exit 2 and nothing left behind"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_fetch.py --selftest (70 cases, ends LAYA FETCH SELFTEST OK; 16 named case families present; 7 mutations each turned it red)"
        status: pass
    human_judgment: false
  - id: D2
    description: "`just laya-fetch` is a registered gate: a fake aws serving one flipped byte is refused (exit 2 REFUSED fetch-sha256, DEST absent, staging removed); an untampered run lands whole; an existing DEST and a foreign profile are refused; the drift check passes"
    verification:
      - kind: integration
        ref: "LAYA_GATES_ONLY=fetch just laya-gates-selftest (DRIFT CHECK OK, ROW fetch GREEN, AWS CALLS: 0, LAYA GATES ROWS OK 1 of 45); the row turned RED with the sha256 check removed; models/decide snapshot cmp-identical before and after"
        status: pass
    human_judgment: false
  - id: D3
    description: "laya_repro_eval.py check-declaration refuses empty or incomplete must, empty or malformed band, attempt_cap below 1, a defect retry class and an unknown waiver rule; evaluate recomputes every clause from the fetched run's own report, verdict and pack-outcome and applies one retry rule for any cap"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest (112 cases, ends LAYA REPRO EVAL SELFTEST OK; 18 named case families present; 10 mutations each turned it red) plus the AST purity scan (imports: ast json math os re shutil sys tempfile)"
        status: pass
    human_judgment: false

duration: 36min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 11: laya-fetch and the pure reproduction evaluator Summary

**A run in S3 becomes a local, sha256-verified directory only whole and only byte-exact (one closed destination mapping validated before any download, one atomic rename), and the D-16 reproduction verdict is a pure, mutation-checked function of the committed declaration and that fetched run, proven offline before any paid run.**

## Performance

- **Duration:** about 36 min (start 07:22Z estimated, finished 07:58Z)
- **Tasks:** 3 (one tracer, two auto; the evaluator task is tdd, built selftest-first within one commit)
- **Files modified:** 4 (two new scripts, `justfile`, `scripts/laya_gates.tsv`)

## Accomplishments

- `laya_fetch.py NAME TASK_ID BUCKET [--dest DIR] [--profile P]`: validates NAME, TASK_ID, BUCKET and the profile (ze-kasher-dev only, `fetch-profile`), reads `manifest.json` first, refuses any entry path that is absolute, has `..`, an empty segment, a backslash or a control character, maps every entry with `dest_for` (manifest.json/verdict.json/`<NAME>.apr` at the root, `run/` and `data/` as they are, other top-level files under `logs/`, any other nested entry `fetch-layout`), refuses duplicate destinations (compared case-insensitively, since macOS is) and file-versus-directory conflicts before any download, then downloads into `.<NAME>.fetch-<pid>`, checks size and sha256 per entry and renames once.
- On success it prints the phase and the `.apr` sha256, a one-line note naming the base snapshot directory and the `hf download` command when that directory is absent, and the owner-run `just laya-verify` and `just laya-deploy` commands (never executed), ending `FETCH OK phase <p> apr <sha|none>`. A gate_failed run with no `.apr` prints no deploy commands.
- `just laya-fetch` is a `[positional-arguments]` recipe delegating to the script, registered as row `fetch` in `scripts/laya_gates.tsv` with `row_fetch` in `laya-gates-selftest`: the real recipe, run against a fake `aws` on PATH, refuses a tampered `gate-report.json` (exit 2, DEST absent, no staging left). Removing the sha256 check turned the row RED.
- `laya_repro_eval.py`: `check-declaration` and `evaluate` as specified. `MUST_KEYS` is a fixed tuple; unknown `must` clauses are refused (they would never be evaluated); band membership is recomputed from the report's top-level `margin` and `fine_tuned.ece_post` (inclusive ends, an absent metric is outside the band); the inputs clause compares every key the declaration declares (the three required ones at minimum) and records every key the report carries; `retry_allowed = class in retry_classes and attempt < attempt_cap` for any cap.
- Neither script can reach the network in its selftest: the fetch selftest swaps the module's subprocess entry point for a tripwire and asserts it never fires (with a control that it fires when called); the evaluator imports no network or subprocess module (checked by an in-selftest AST scan and by the plan's own scan).

## Task Commits

1. **Task 1 (tracer): laya_fetch.py through a fake aws runner** - `9ff89bb1e` (feat)
2. **Task 2: the `laya-fetch` recipe and its gate row** - `e3115b8f8` (feat)
3. **Task 3: the pure reproduction evaluator** - `6ab0e10e5` (feat)

**Plan metadata:** the docs commit that carries this file (docs(14-11)).

## Files Created/Modified

- `scripts/laya_deploy/laya_fetch.py` - the fetch, `dest_for`, `AwsRunner`, `FixtureRunner`, 70-case `--selftest`
- `scripts/laya_deploy/laya_repro_eval.py` - `check_declaration_obj`, `evaluate_run`, `MUST_KEYS`, 112-case `--selftest`
- `justfile` - `laya-fetch` recipe (after `laya-prepare-motivation`) and `row_fetch` in `laya-gates-selftest`
- `scripts/laya_gates.tsv` - row `fetch`

## Decisions Made

- The evaluator reads `phase` from `verdict.json` top level and `waived` from `verdict.waived`, because 14-05's finalizer stores the status payload byte for byte as `verdict.json`.
- The seeds clause compares against the declaration's `must.seeds` (which 14-14 writes as 13, 17, 23), not a literal, so the declaration stays the single place the owner approves.
- A gate_failed run that fetched no `.apr` is a successful fetch (`FETCH OK phase gate_failed apr none`); the reports are the product.

## Deviations from Plan

### Auto-fixed / extended items

**1. [Rule 1 - Bug, in this plan's own code] A pack-outcome.json holding JSON `null` read as "absent, no reason"**
- **Found during:** Task 3 selftest (the junk-file case)
- **Issue:** `read_json_file` returned `(None, None)` for `null`, so a passing report beside a `null` pack-outcome produced no defect reason and could have been classified `pass`.
- **Fix:** any file that is not a JSON object now yields an `unreadable-file` reason; a dedicated case pins it.
- **Files modified:** `scripts/laya_deploy/laya_repro_eval.py`
- **Commit:** `6ab0e10e5`

**2. [Rule 2 - Missing critical] Extra refusals and defect rules beyond the plan's list**
- **Fetch:** `fetch-size` (size mismatch, distinct from `fetch-sha256`), `fetch-bucket`, `fetch-usage`; case-insensitive duplicate detection; a symlink or file DEST refused; a manifest that lists itself, lists nothing, lacks `verdict.json`, has an extra entry key, a duplicate JSON key, a bad size or a bad sha256 refused; a printed `NOTE` when `unlisted > 0` (objects the manifest does not cover are not fetched).
- **Evaluator:** `phase-disagreement`, `waiver-disagreement`, `waiver-on-passing-gate`, `pack-accepted-without-waiver` and `pack-refused-under-waiver` defects, symlinked inputs refused, NaN literals refused at parse, unknown `must` or `band` keys refused, a declared `must.inputs_sha256` key beyond the three required ones is compared.
- **Why:** each closes a way for the verdict to read a vacuous or contradictory run as acceptable (T-14-75, T-14-27).
- **Commits:** `9ff89bb1e`, `6ab0e10e5`

**3. [Plan wording] "seeds 13/17/23" is implemented as "exactly the declaration's must.seeds"**
- The plan's behaviour text and 14-14's declaration agree on 13/17/23; reading the declaration keeps one source of truth. The selftest uses 13/17/23.

**4. [Test construction] The conflict case does not store `run/a` in the fixture bucket**
- A filesystem cannot hold `run/a` and `run/a/b`; S3 can. The manifest lists both and the fetch refuses before any get, so the object for `run/a` is omitted from the fixture (the case asserts no get ran).

**Total deviations:** 4 (1 bug in own code, 1 missing-critical bundle, 2 interpretation or construction notes). **Impact:** none on scope; every addition is a refusal or a test.

## Issues Encountered

- A formatter hook rewrote both scripts after each write (the files grew from about 600 to 766 and about 650 to 842 lines); no behaviour effect. My first mutation-check sed patterns silently failed to match after that; redone with exact-string replacement in Python, after which all 7 (fetch) and 10 (evaluator) mutations turned the selftests red.
- `fetch-name: leading hyphen` cannot be tested through the CLI (`-x` is read as an option and refused `fetch-usage`); that case calls `check_inputs` directly.

## Known Stubs

None. (`laya-fetch` prints `SERVER` as a literal placeholder in the printed `laya-deploy` command by design: the owner names the server.)

## Threat Flags

None. No new endpoint or auth path. The fetch's only external call is `aws s3api get-object` via argv (never a shell), run by the owner; this plan made no AWS call and no push.

## User Setup Required

None.

## Next Phase Readiness

- 14-14 runs `laya_repro_eval.py check-declaration` on its committed declaration (schema `laya-reproduction-declaration-v1`: `must`, `band`, `retry.{attempt_cap, retry_classes}`, top-level `waiver_rule`); its `must` must hold exactly `deploy_eligible`, `device_prefix`, `recipe_id`, `inputs_sha256` (task_json, train_jsonl, eval_jsonl at least) and `seeds`, and `retry_classes` may only name `in-band-gate-fail` and `waived`.
- 14-15 fetches each attempt with `just laya-fetch` and records `evaluate`'s one JSON line per attempt (it also carries a `recorded` block with every inputs key, the seeds and the device); its verify re-runs `evaluate` and compares the parsed line.
- The 14-14/14-15 gates that run `just laya-fetch` against real S3 need the `ze-kasher-dev` profile; this plan never exercised it.
- `python3 -I scripts/laya_deploy/laya_fetch.py --selftest` and `... laya_repro_eval.py --selftest` run offline in about a second each.

## Self-Check: PASSED

Verified: `scripts/laya_deploy/laya_fetch.py` and `laya_repro_eval.py` exist; the three task commits (`9ff89bb1e`, `e3115b8f8`, `6ab0e10e5`) are ancestors of HEAD; the plan's three `<verify>` scripts pass (fetch selftest 70 ok / 0 FAIL, 16 named families present; `LAYA_GATES_ONLY=fetch just laya-gates-selftest` with `DRIFT CHECK OK` and `LAYA GATES ROWS OK`, models/decide snapshot unchanged, `just laya-fetch --selftest` ends `LAYA FETCH SELFTEST OK`, `fetch` row present; evaluator selftest 112 ok / 0 FAIL, 18 named families present, AST scan prints PURE EVALUATOR).
