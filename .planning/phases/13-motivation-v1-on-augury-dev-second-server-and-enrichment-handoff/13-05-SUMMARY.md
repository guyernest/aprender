---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 05
subsystem: laya-data-prep
tags: [laya, motivation-v1, selection, d-17, d-14, d-03, d-16, seeded-sampling, no-text, contract-mirror]

requires:
  - phase: 13-02
    provides: "contracts/motivation-enrichment-v1.yaml: motivation-record-v1 fields with structured patterns, sum_tolerance, length, range, max_chars and this_round values; the enrichment selection_slice enum and caps.rows"
  - phase: 13-03
    provides: "prepare_motivation.py's _under_repo and _object_no_duplicates (reused, OPS-03), the d15_amendment as implemented, and the source-<k> / sentinel / capture-control selftest style"
provides:
  - "scripts/laya_train/select_motivation.py: the seeded D-17 selection rule over motivation-record-v1 files, its validator (13 select-* refusals), --print-rule and a 107-case --selftest"
  - "output format motivation-selection-v1: a header (seed, budget, shares, quota_split, band, targets, control_population, records [{id, sha256}], counts incl. band_share_of_pool and shortfall) and {post_id, selection_slice, rank_key} rows"
  - "13-SELECTION-NOTE.md: the partner-facing selection guidance, its rule region the verbatim --print-rule output"
affects: [13-10]

actuals:
  tokens: 21500
  tasks: 2
  commits: 3
plan_head_before: 822523b663d6420ff3d1f9e2d425d1e634e702f0
plan_head_after: 10bab5dd631e789097c37255548156d9d91eed00

tech-stack:
  added: []
  patterns:
    - "Probabilities parsed as exact decimals (json parse_float=Decimal), so band edges and the sum tolerance compare exact values; 'off by exactly the tolerance' is a well-defined accept"
    - "A note's rule section generated from the script's constants (--print-rule) and compared byte for byte; prose may not state a number the generated region does not hold"
    - "Order-independent seeded selection: rank = sha256('<seed>:<post_id>'), so shuffled input selects the same rows; only the header's input sha256 differs"

key-files:
  created:
    - scripts/laya_train/select_motivation.py
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-SELECTION-NOTE.md
  modified: []

key-decisions:
  - "Probabilities are read as exact decimals, not binary floats: the plan's boundary cases (a sum off by exactly sum_tolerance accepted, margin exactly 0.20 not eligible) are only well defined that way"
  - "With three labels summing to one the band's margin clause adds no post (p1 < 0.45 forces p2 > 0.275; p1 > 0.75 forces a margin above 0.5), so the band is p1 <= 0.75 in practice. Kept as declared; stated in the docstring and the note, proven by a 3000-vector selftest case"
  - "--budget is bounded 1..20000, the motivation-enrichment-v1 caps.rows the expert's file can hold, mirrored from the contract"
  - "shortfall is a flat map (control, uncertainty, quota_<label>) so its values sum to the total gap; per_slice and per_predicted_label_in_quota are reported beside it"
  - "The selection's slice names are mirrored against the enrichment row selection_slice enum (control, uncertainty, quota), so a returned enrichment file can echo them; the note states the d15_amendment as pending, consistent with prepare_motivation.py as committed"

patterns-established:
  - "Generated rule region: <!-- selection-rule:begin --> / <!-- selection-rule:end --> holds --print-rule stdout verbatim"

requirements-completed: [D-17, D-14, D-03, D-16]

coverage:
  - id: D1
    description: "The seeded selection rule (control first, then uncertainty band, then quota by predicted label; no backfill; dedupe; exclusion) is implemented once, deterministic under shuffling, disjoint and text-free"
    requirement: D-17
    verification:
      - kind: unit
        ref: "uv run --project scripts/laya_train --frozen python -I scripts/laya_train/select_motivation.py --selftest (107 ok, 0 FAIL)"
        status: pass
      - kind: integration
        ref: "13-05-PLAN.md Task 1 <automated>: 1500 synthetic records -> SELECTION RUN OK {control 75, uncertainty 120, quota 105}"
        status: pass
    human_judgment: false
  - id: D2
    description: "motivation-record-v1 validation bound to the contract's structured values, with boundary cases read from the contract and 13 text-free refusals"
    requirement: D-14
    verification:
      - kind: unit
        ref: "select_motivation.py --selftest: contract mirror (10 cases), boundary (11 cases), every refusal rule (13 names, no other)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The selection output carries post_id, selection_slice and rank_key only: no text, probability or label"
    requirement: D-03
    verification:
      - kind: unit
        ref: "select_motivation.py --selftest: 'rows hold post_id, selection_slice, rank_key only' and the sentinel / file-name cases"
        status: pass
    human_judgment: false
  - id: D4
    description: "13-SELECTION-NOTE.md: the rule region equals --print-rule byte for byte, the prose restates no other number, and the required statements are present"
    requirement: D-17
    verification:
      - kind: other
        ref: "13-05-PLAN.md Task 2 <automated>: SELECTION NOTE OK 22 numbers generated"
        status: pass
    human_judgment: false
  - id: D5
    description: "The note is clear and usable for the partner (tone, whether the three-slice rationale and the --no-exclude choice read correctly to a non-aprender reader)"
    requirement: D-16
    verification: []
    human_judgment: true
    rationale: "Readability for the partner and the owner is a judgement no check asserts; the label-first suggestion (D-16) is advice whose usefulness depends on the partner's sheet layout"

duration: 15min
completed: 2026-10-27
status: complete
---

# Phase 13 Plan 05: Selection rule and selection note Summary

**`select_motivation.py` implements the D-17 rule once over validated motivation-record-v1 files. It is seeded and order-independent: rank = sha256 of `<seed>:<post_id>`. It draws a uniform control first (75), then the uncertainty band (120), then a quota by predicted label (45/30/30) at budget 300, with no backfill. It writes a text-free motivation-selection-v1 file. `13-SELECTION-NOTE.md` embeds its `--print-rule` output verbatim, so the prose and the code cannot diverge.**

## Performance

- **Duration:** about 15 min
- **Started:** 2026-10-28T01:26:23Z (UTC; 2026-10-27 local)
- **Completed:** 2026-10-28T01:41:45Z
- **Tasks:** 2
- **Files created:** 2

## Accomplishments

- **Selftest: 107 ok and 0 FAIL**, ending `SELECT MOTIVATION SELFTEST OK`. It includes cases named for shuffled input, the all-band pool, the file name, boundaries, the contract mirror and print-rule.
- **The plan's synthetic run** used 1500 records generated by the verify script at budget 300. It gave `SELECTION RUN OK {'control': 75, 'uncertainty': 120, 'quota': 105}`, with every shortfall 0 and band_share_of_pool 0.552667. The header names `source-1` and states the bounded `control_population`. The sentinel and the record file's name appear in neither the output nor the log.
- **The selftest's 2000-record run** (budget 300) gave 75 / 120 / 105, with the quota at 45 / 30 / 30. Further cases:
  - **Budget 100:** 25 / 40 / 35, with the quota at 15 / 10 / 10.
  - **Thin pool:** 6 conspiratorial posts were left for the quota and all 6 were taken. The shortfall of 39 is recorded and 261 rows are written, with no backfill.
  - **All-band pool:** band_share_of_pool is 1.0. Control is still the 75 smallest rank keys, uncertainty is the next 120, and the quota is filled from the rest.
- **Validator and refusals:** 13 `select-*` refusals, each produced through `main()` with exit 2. Each names the source id, the row and the field, and none prints the sentinel. A coverage case confirms that no other refusal name escapes.
- **Contract mirror:** the selftest compares the record keys, the deployment keys, the labels, all 8 patterns, the probability length, range and sum_tolerance, the text bounds, tokens min, the artifact sha, the selection_slice enum and caps.rows with the contract's structured values.
- **Mutation spot check** (restored and `git diff --quiet` afterwards). Each of 8 mutations turned the selftest red:
  - band `<` to `<=`;
  - uncertainty drawn before control;
  - dedupe keeping the larger rank key;
  - a refusal echoing the text;
  - the header naming the file;
  - a quota backfill;
  - a widened tolerance;
  - the quota numerator 3 changed to 4.
- **Note check:** the Task 2 verify printed `SELECTION NOTE OK 22 numbers generated`. Two negative controls failed as they should: a hand edit inside the region (75 changed to 76) is caught as "region differs", and a prose "41 of 295" is caught as extra numbers.

## Task Commits

1. **Task 1 (tracer) RED:** `32927e495` test(13-05): failing selftest for the seeded motivation selection rule
2. **Task 1 (tracer) GREEN:** `b3a522a62` feat(13-05): the seeded motivation selection rule over motivation-record-v1 files
3. **Task 2:** `10bab5dd6` docs(13-05): the selection note for the partner, with the rule region generated by --print-rule

The tracer gate: auto mode is off, the verify mode is end-of-phase and the verify is automated only. The Task 1 verify was re-run green at `b3a522a62` before Task 2 started (`tracer verify rc=0`).

## TDD (`tdd="true"` on Task 1; `workflow.tdd_mode` is false, so the gate is advisory)

- **RED** `32927e495`: the selftest exited 1, with 92 FAIL and 15 ok.
  - The rule functions were neutral stubs: an empty selection, `False` for the band, exit 1 for a run. Every behaviour case therefore failed on its planned assertion, with no crash and no fixture fault.
  - The 15 that passed were the 10 contract-mirror cases (the constants already existed), the capture control, the "nothing written in the repository" check, and 3 NOT-eligible band edges that a stub returning `False` satisfies vacuously.
  - One selftest fault was fixed before the RED commit. A nested section reassigned `DEFAULT_EXCLUDE` without a `global` declaration (UnboundLocalError). That was a test bug, not an intended RED.
  - `gsd-tools check tdd-red-evidence` was not run. Its adapters do not read this selftest's `ok`/`FAIL` lines, the same limitation 13-01 and 13-03 recorded.
- **GREEN** `b3a522a62`: 107 ok, 0 FAIL, and the plan's Task 1 verify passes.
- **REFACTOR:** none.

## Files Created/Modified

- `scripts/laya_train/select_motivation.py` (created). It contains:
  - the declared rules in the docstring;
  - the contract-mirrored constants and the 13 refusals;
  - `read_records`, `known_identities`, `select`, `render`, `rule_values` and `rule_markdown`;
  - `--print-rule` and `--selftest`.
- `.planning/phases/13-.../13-SELECTION-NOTE.md` (created). Sections 1, 1b and 2 to 7, with the generated rule region between the `selection-rule` markers.

## Decisions Made

See `key-decisions` in the frontmatter. Two of them are worth reading before the 13-10 hand-off:

- **Exact decimals.** The plan's boundaries (a sum off by exactly the tolerance, a margin of exactly 0.20) are only well defined in exact arithmetic, so probabilities are parsed as `Decimal`.
- **The band's margin clause is inert for three labels.** The band is effectively p1 <= 0.75. That follows from the declared rule, so the rule was not changed. It is stated in the note and in the docstring, and a selftest case proves it. If the owner wants the margin clause to bite, that is a rule change: new constants and a re-generated note.

## Deviations from Plan

### Additive (inside the declared refusal names and the contract)

- **Header key `targets`.** Added beside the planned header keys, so a reader sees the per-slice targets next to the shortfall.
- **Shortfall shape.** `shortfall` covers control and uncertainty as well as the three quota labels. The plan's verify sums all its values; with control and uncertainty met, that sum is the quota gap.
- **Extra refusal paths** that the refusal meanings imply:
  - a directory passed as `--records` (select-missing);
  - a symlink pointing into the repository (select-in-repo);
  - NaN or Infinity, and invalid UTF-8 (select-json);
  - an `--exclude-data` dir without eval.jsonl (select-exclude-missing);
  - a non-integer `--budget` (select-budget).
- **Reused helpers.** `_under_repo` and `_object_no_duplicates` are imported from `prepare_motivation.py` (OPS-03) instead of being copied. The file itself is unmodified.

**Total deviations:** 0 auto-fixed. **Impact:** none on scope.

## Issues Encountered

- The first draft of the note wrote "RFC 3339" in the prose. The Task 2 numbers check flagged `3339`, a number outside the generated region. It was reworded to "a UTC timestamp ending in Z, as the contract's `scored_at` pattern declares".

## Known Stubs

None. The RED stubs were replaced by the implementation in `b3a522a62`.

## User Setup Required

None. No live AWS or pmcp.run call was made. Nothing under `models/decide/` was touched, and neither were justfile, laya_gates.tsv or prepare_motivation.py.

## Next Phase Readiness

- Plan 13-10 can point the partner to `13-SELECTION-NOTE.md` and the `select_motivation.py` command. It must still put the `d15_amendment` to the owner. The note states that amendment as pending.
- No real record file exists yet. The first one arrives when the partner scores its stored posts on the augury-dev deployment.

## Self-Check: PASSED

- FOUND: scripts/laya_train/select_motivation.py
- FOUND: .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-SELECTION-NOTE.md
- FOUND: 32927e495, b3a522a62, 10bab5dd6 (ancestors of HEAD)
- Task 1 verify rc 0 and Task 2 verify rc 0, both re-run after the last task commit
