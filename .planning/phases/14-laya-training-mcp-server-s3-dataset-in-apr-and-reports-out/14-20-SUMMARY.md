---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 20
subsystem: testing
tags: [laya, drift, pack-verify, cross-cpu, d-15, gap-closure, bashrs, s3-retention, fake-aws]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-14's smoke run and recorded Mac/Graviton figures (14-SMOKE-EVIDENCE.json), 14-15's motivation-v1-sm run and Graviton refusal (14-GATE-RUN-EVIDENCE.json), the fetched run dirs under models/decide/, 14-19's justfile and laya_gates.tsv conventions"
provides:
  - "scripts/laya_deploy/laya_drift.py: subcommands local, aws-plan, collect, bashrs-gate and --selftest (83 cases)"
  - "14-DRIFT-LOCAL-EVIDENCE.json (laya-drift-evidence-v1): 5 local legs on an Apple M4 Pro, the pinned binary, positive, negative and thread controls, A10G noise references"
  - "models/laya-trainer/drift-graviton.sh (gitignored): the owner-run Graviton script for 14-21, generated, bashrs-gate PASS"
  - "just laya-drift and gate row drift"
affects: [14-21, 14-22]

actuals:
  tokens: 39169
  tasks: 3
  commits: 3
plan_head_before: 0a9ff6635e68472cf2c415c75bcf58994783914c
plan_head_after: fa3fbd2ffdc07bf1615e7c2e2084ac209dc00bba

tech-stack:
  added: []
  patterns:
    - "pinned measurement binary: path from cargo's compiler-artifact message, sha256 and commit recorded, a dirty cargo-tree closure refused"
    - "a fake aws that models S3 tag semantics (copy inherits source tags unless REPLACE) with a keep-seeded source, judged by one oracle over arguments, end state and read-back order"
    - "bashrs judgement for a gitignored file: copy into a temporary git repo and run the repository gate over it"

key-files:
  created:
    - scripts/laya_deploy/laya_drift.py
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-LOCAL-EVIDENCE.json
  modified:
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The dirty check covers the whole in-tree closure of aprender-decide (8 crates from cargo tree), the two contracts pack_laya reads at run time and Cargo.lock, not only crates/aprender-decide"
  - "The generated Graviton script names the task definition as family:revision; the full ARN literal trips bashrs SEC005 (task-definition reads as an sk- key)"
  - "aws-plan writes the script only after the static policy table and bashrs-gate pass, so an unlintable or policy-violating script never lands"
  - "The negative control uses the smoke artifact with motivation-v1-sm's data directory; it was refused by the Pack rule (task.json is not a byte copy), recorded not_a_drift_leg"
  - "Thread control: engaged (x4.97 wall time from 1 to 8 threads) with an identical re-score at 1, 4, 8 and default, so on this CPU the rayon thread count does not move the numbers"

patterns-established:
  - "Drift legs are judged by their last PACKED / verify-JSON / REFUSED line, and the line class must agree with the child's return code"

requirements-completed: [D-05, D-14, D-15, D-17]

coverage:
  - id: D1
    description: "Tracer: one leg on the pinned binary reproduces the recorded Mac refusal of the smoke artifact exactly"
    requirement: D-15
    verification:
      - kind: integration
        ref: "python3 -I scripts/laya_deploy/laya_drift.py local --only smoke-stance-64 --repeats 1 --out models/decide/drift-tracer -> POSITIVE CONTROL reproduced, DRIFT LOCAL OK legs=1 (run twice, 106.4 s and 109.2 s)"
        status: pass
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_drift.py --selftest#parser case table (24 rows), dirty-closure refusal, fake pack_laya legs"
        status: pass
    human_judgment: false
  - id: D2
    description: "Owner-run Graviton script generator: every write is under runs/drift- and expire-tagged, the read-back runs before run-task, the script is lint-clean"
    requirement: D-14
    verification:
      - kind: unit
        ref: "laya_drift.py --selftest#clean run against the tag-modelling fake aws, three tag mutants refused by the oracle, the static table and the script's own read-back, bashrs-gate case table, collect scrub"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_drift.py bashrs-gate models/laya-trainer/drift-graviton.sh -> BASHRS GATE PASS"
        status: pass
    human_judgment: false
  - id: D3
    description: "The local legs with controls and repeats, as committed evidence holding no account id or ARN"
    requirement: D-15
    verification:
      - kind: other
        ref: "the plan's Task 3 evidence verify -> LOCAL EVIDENCE OK (5 legs)"
        status: pass
      - kind: other
        ref: "git diff --quiet HEAD -- contracts crates/aprender-decide (rc 0)"
        status: pass
    human_judgment: false
  - id: D4
    description: "Recipe laya-drift and gate row drift"
    requirement: D-17
    verification:
      - kind: integration
        ref: "LAYA_GATES_ONLY=drift just laya-gates-selftest -> LAYA GATES ROWS OK 1 of 56, AWS CALLS: 0"
        status: pass
      - kind: other
        ref: "red side: the table without the drift row -> DRIFT: laya-drift ... neither a row target nor a not-a-gate row"
        status: pass
    human_judgment: false
  - id: D5
    description: "The generated Graviton script does what its header says against the real account (run in 14-21)"
    verification: []
    human_judgment: true
    rationale: "Only the fake aws has run it; the real copy-object, tag read-back and Fargate pack task are 14-21's owner-run legs"

duration: 72min
completed: 2026-10-30
---

# Phase 14 Plan 20: local cross-CPU drift legs and the Graviton script Summary

**The re-score drift is now measured on this Mac rather than argued. The pinned `pack_laya` binary reproduces the recorded smoke refusal exactly: fine_tuned row 199, max_abs 3.439e-5 against the bound 3.259e-5. That result is identical over 3 repeats and at 1, 4, 8 and default rayon threads, and the knob is engaged (×4.97 wall time from 1 to 8 threads). motivation-v1-sm also drifts on this Mac, at zero_shot row 17 with 1.490e-5 against the 1e-5 floor; Graviton recorded 1.729e-5 on the same row. The three MPS-trained artifacts are accepted. The owner-run Graviton script for 14-21 is generated and lint-clean, and a fake aws that models tag inheritance shows that every object it writes ends up tagged laya-retention=expire.**

## Performance

- **Duration:** 72 min (local legs about 50 min of that)
- **Started:** 2026-10-30T18:30:12Z
- **Completed:** 2026-10-30T19:42:01Z
- **Tasks:** 3
- **Files modified:** 4

## Results (this Mac: Apple M4 Pro, arm64, 14 cores; binary sha256 64baa3f2..., commit f0204a968, clean closure)

| leg | trained on | step | set | max_abs | bound | / bound | / training noise | repeats | identical |
|---|---|---|---|---|---|---|---|---|---|
| smoke-stance-64 | cuda (A10G) | verify | fine_tuned (refused, row 199) | 3.4392e-5 | 3.2590e-5 | 1.055 | 4.22 | 3 (+4 thread settings) | yes |
| motivation-v1-sm | cuda (A10G) | pack | zero_shot (refused, row 17) | 1.4901e-5 | 1e-5 (floor) | 1.490 | 8.63 | 3 | yes |
| laya-motivation-v1 | mps | verify | fine_tuned / zero_shot | 9.15e-6 / 3.34e-6 | 7.28e-5 / 5.01e-5 | 0.126 / 0.067 | 0.50 / 0.27 | 1 | - |
| laya-stance-64 | mps | verify | fine_tuned / zero_shot | 1.13e-5 / 7.35e-6 | 3.34e-5 / 5.98e-5 | 0.340 / 0.123 | 1.36 / 0.49 | 1 | - |
| laya-antisemitism-v1 | mps | verify | fine_tuned / zero_shot | 7.55e-6 / 1.20e-5 | 1.44e-5 / 6.03e-5 | 0.526 / 0.198 | 2.10 / 0.79 | 1 | - |

Recorded Graviton figures for the same bytes: smoke fine_tuned 2.748e-5 (accepted), motivation zero_shot 1.729e-5 on the same row 17 (refused). In both cases the two aarch64 CPUs disagree. The direction is not consistent: the Mac figure is higher for smoke and lower for motivation. Every MPS data directory was found by input hash (data/decide/motivation-v1, tweet-stance-64 and goldstandard-2024), so none was not measurable. A refusal reports only the first drifted set, so each refused leg measures one set.

What the thread control shows is narrow. On this CPU the rayon thread count does not change the re-score. Running with 8 threads does not reproduce Graviton's 8-vCPU figure, so that is not why Graviton differs. No other cause is named.

## Accomplishments

- **Task 1 (tracer):** `laya_drift.py local`.
  - The executable path comes from cargo's compiler-artifact message: `target/release/examples/pack_laya`.
  - A dirty closure is refused before any build.
  - Each child runs from an explicit argv with its output going to files, and the rc is read from the child object.
  - The parser has a 24-row case table.
  - The positive control was reproduced, and the tracer verify was re-run for the feedback gate with an identical result.
- **Task 2:** `aws-plan`, `bashrs-gate`, `collect` and their selftest.
  - **Fake aws.** It is seeded with a keep-tagged smoke source and an expire-tagged motivation source.
  - **Clean run.** Asserted: the call order (sts first, describe-task-definition second), the profile and region on every call, only allowed verbs, 12 writes all under runs/drift- with the sources byte- and tag-identical afterwards, and only run/ and data/ copied. The oracle finds no violation, and the results were fetched.
  - **Gates.** Each of these is refused before any call: no approval, a wrong account (only the sts call is made), an unknown leg, and a bad repeat count.
  - **Tag mutants.** Each of the three is refused by the oracle for its own reason, by the static table, and by the script's own read-back, before any run-task. The fake's inheritance model matters: with it removed, the selftest goes red.
  - **Static table and lint.** The static table has 10 refusal cases, and a comment that spells a refused shape is ignored. bashrs-gate passes the generated script and refuses an appended unguarded `rm -rf "$work"`. Its case table runs under bashrs 6.66.3 and takes its SEC010 and DET002 fixtures verbatim from the gate.
  - **Collect.** collect scrubs a planted account id and an ARN, and refuses to write when an id is planted as a key.
- **Task 3:** all local legs and controls, `14-DRIFT-LOCAL-EVIDENCE.json`, `just laya-drift`, and gate row `drift`.

## Task Commits

1. **Task 1 (tracer): drift harness local leg on the pinned binary** - `29c6f7367` (feat)
2. **Task 2: Graviton script generator, bashrs-gate and collect** - `f0204a968` (feat)
3. **Task 3: local legs, controls, recipe and gate row** - `fa3fbd2ff` (feat)

## Files Created/Modified

- `scripts/laya_deploy/laya_drift.py` - the harness (2119 lines, stdlib only)
- `.planning/phases/14-.../14-DRIFT-LOCAL-EVIDENCE.json` - the local legs, controls and references
- `justfile` - `laya-drift` recipe and `row_drift`
- `scripts/laya_gates.tsv` - row `drift`
- gitignored, not committed: `models/laya-trainer/drift-graviton.sh` (holds the account id), `models/decide/drift-tracer{,.run1}/`, `models/decide/drift-local/`

## For 14-21 (owner-run, not run here)

```
LAYA_DRIFT_APPROVED=graviton bash models/laya-trainer/drift-graviton.sh smoke:1                # tracer stage
LAYA_DRIFT_APPROVED=graviton bash models/laya-trainer/drift-graviton.sh smoke:2 motivation:3   # full stage
python3 -I scripts/laya_deploy/laya_drift.py collect   # -> 14-DRIFT-EVIDENCE.json and 14-DRIFT-TABLE.md
```

Regenerate the script with `just laya-drift aws-plan --source smoke:3e964ab5-a1ca-458b-bcec-22e094f2f2a6 --source motivation:a2cbacae-529c-4703-a796-b6eda6742446` if outputs-dev.json changes.

## Decisions Made

See key-decisions in the frontmatter.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] The dirty check covers the binary's whole source closure**
- **Found during:** Task 1
- **Issue:** Checking only crates/aprender-decide would let a dirty aprender-core or aprender-compute (the ModernBERT and GEMM code), or a dirty contract the binary reads at run time, change the numerics while the record still said clean.
- **Fix:** The check now covers the cargo-tree path crates plus both Laya contracts and Cargo.lock. A path crate outside the checkout is refused.
- **Committed in:** 29c6f7367

**2. [Rule 3 - Blocking] Two bashrs findings in the first template**
- **Found during:** Task 2
- **Issue:** SEC005 fired on the task-definition ARN literal, and SEC010 fired on `mkdir -p "$work"`. Measured: a `case` guard does not clear SEC010, but a `[[ "$work" != *..* ]]` test does.
- **Fix:** The task definition is passed as family:revision, and an absolute-path guard and a no-`..` guard in `[[ ]]` form come before the mkdir.
- **Committed in:** f0204a968

**3. [Rule 2 - Missing critical] Fail-closed additions to the generated script and to aws-plan**
- **Found during:** Task 2
- **Issue:** Nothing stopped a script that failed its own static table or the bashrs gate from being written. Nothing guarded against writing into a non-empty measurement prefix or reusing a results directory.
- **Fix:** aws-plan writes the script only after the static table and bashrs-gate pass. The script checks that the new prefix is empty (`list-objects-v2`) before any write, and numbers each results directory LEG-N without overwriting.
- **Committed in:** f0204a968

**4. [Rule 3 - Disk] The scratch `.apr` from each pack leg is removed after hashing**
- **Found during:** Task 3
- **Issue:** The disk was 96% full (39 GiB free), and each packed `.apr` is 807 MB.
- **Fix:** A pack leg compares the file's sha256 with its PACKED line and then removes the file. In this run all three motivation-v1-sm pack legs refused, so nothing was written.

**Total deviations:** 4 auto-fixed (2 Rule 2, 2 Rule 3). **Impact:** all of them make the harness stricter; none changes what it measures or claims.

## Issues Encountered

- **Tracer re-run:** the tracer verify's fixed `--out models/decide/drift-tracer` refuses a non-empty directory. For the feedback-gate re-run, the first run's directory was moved aside to `drift-tracer.run1`, which is gitignored.
- **Output buffering:** Python buffers stdout to a file, so the background log showed nothing until the run finished. The run was judged by its last line, `DRIFT LOCAL OK legs=5`.
- **rtk hook:** it rewrites `grep` and `git diff` output in the Bash tool. actuals.tokens was therefore measured from the files and an in-Python `git diff`.

## State and roadmap updates

- The plan's requirement IDs (D-05, D-14, D-15, D-17) are phase decision IDs, not REQUIREMENTS.md entries, and the plan says "Requirement-ID probes: SKIPPED visibly". `requirements.mark-complete` is therefore not run.
- Windows ledger: no stub, skipped test or unrun verify was left, so nothing was appended.
- STATE.md and ROADMAP.md updates are recorded below after they run.

## User Setup Required

None. No AWS call was made, nothing was deployed, and no contract was amended.

## Next Phase Readiness

- 14-21 can run the Graviton legs with the generated script, then `collect`.
- Open for the brief: the x86_64 CPU (no pack binary or image exists for it), and the unmeasured second set in each refused leg.

## Self-Check: PASSED

- Files: `scripts/laya_deploy/laya_drift.py`, `14-DRIFT-LOCAL-EVIDENCE.json` and `models/laya-trainer/drift-graviton.sh` all exist.
- Commits: `29c6f7367`, `f0204a968` and `fa3fbd2ff` are ancestors of HEAD. Measured: `git rev-list --count 0a9ff6635..HEAD` = 3.
- Plan verification, re-run after the last task commit:
  - `--selftest`: DRIFT SELFTEST OK (83 cases);
  - the evidence verify: LOCAL EVIDENCE OK (5 legs);
  - `LAYA_GATES_ONLY=drift just laya-gates-selftest`: LAYA GATES ROWS OK 1 of 56;
  - `git diff --quiet HEAD -- contracts crates/aprender-decide`: rc 0.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
