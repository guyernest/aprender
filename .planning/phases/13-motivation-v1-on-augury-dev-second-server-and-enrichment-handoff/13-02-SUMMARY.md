---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 02
subsystem: contracts
tags: [pv, schema-contract, motivation-v1, enrichment, laya, ontology-ratchet]

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "the served motivation-v1 task (task.json f74a3ad3...), artifact d202f495..., the v1 train/eval pins and prepare_motivation.py"
provides:
  - "contracts/motivation-enrichment-v1.yaml: kind schema 1.0.0 declaring motivation-record-v1 (D-14) and motivation-enrichment-v1 (D-13, D-15, D-16) with structured bounds"
  - "the D-15 (label, evidence) mapping table with all 12 cells, the d15_amendment block, the five D-11/D-12 merge rule names in order, 17 enrichment-* refusal names, the no-text rule and the D-16 anchoring statement"
  - "regenerated contracts/census.json and contracts/contracts.nt (shapes.ttl unchanged)"
affects: [13-03, 13-05, 13-10]

actuals:
  tokens: 4000
  tasks: 2
  commits: 4
plan_head_before: 50a3f58bba76c6a10174b016e9de590854545060
plan_head_after: 77fcf488f71b7e3a1c913db5a051323de7b2ef93

tech-stack:
  added: []
  patterns:
    - "Partner-facing data formats are declared as a kind schema contract before any file exists; every enforced bound is a structured value a mirror check can compare"
    - "Inside contracts/, a key named file, path or source_file makes a contract count as unanchored but bindable (ONT R-5); name output file names something else"

key-files:
  created:
    - contracts/motivation-enrichment-v1.yaml
  modified:
    - contracts/census.json
    - contracts/contracts.nt

key-decisions:
  - "OWNER DECISION PENDING (d15_amendment): the enrichment evidence enum adds `none` (an other post with no evidence) beyond D-15's clear/weak/mixed, and refuses other+clear, social_affiliative+none and conspiratorial+none as enrichment-label-evidence; evidence means the evidence for the RECORDED function. Status: pending the owner's confirmation in the 13-10 hand-off, before the partner builds its sheet; a change is motivation-enrichment-v2"
  - "valid_under {world: committed} was KEPT: the ONT-7 test (13 passed) and the corpus-wide valid-under gate (0 errors) accept it on this schema-kind contract"
  - "The manifest's output file name is declared under `filename`, not `file`: a `file:` key made the ontology ratchet's unanchored_but_bindable counter rise 297 to 298 (FAIL); the contract wording changed, the baseline did not"
  - "The mapping is a structured list of 12 (label, evidence) cells, each with `served` or `refused: enrichment-label-evidence`, so plan 13-03 can mirror it cell for cell; refusals are {name, meaning} objects"

patterns-established:
  - "Schema contract for a data format: closed key sets, per-field type plus pattern / min_chars / max_chars / enum / const / equals / length / range / sum_tolerance / min, integer caps, unique keys"

requirements-completed: [D-13, D-14, D-15, D-16, D-11, D-12, D-01, D-03]

coverage:
  - id: D1
    description: "The motivation-enrichment contract declares both formats, the served task pins, the mapping, merge rules and refusal names, and pv validate accepts it"
    requirement: D-13
    verification:
      - kind: other
        ref: "13-02-PLAN.md Task 1 <automated>: pv validate (0 error(s), 0 warning(s)) plus the pyyaml probe script (CONTRACT DECLARATION OK 17 refusals, 10 structured patterns)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The corpus stays consistent: contract lint PASS, census equals a fresh census, extract --check passes in a clean worktree of HEAD, ratchet PASS with the baseline unchanged, ONT-7 and README contract tests green"
    requirement: D-13
    verification:
      - kind: other
        ref: "13-02-PLAN.md Task 2 <automated> (rc 0 at 77fcf488f)"
        status: pass
      - kind: integration
        ref: "cargo test -p aprender-contracts-cli --test ont7_valid_under_gate (13 passed)"
        status: pass
      - kind: integration
        ref: "cargo test -p aprender-core --test readme_contract (16 passed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The d15_amendment (evidence level none, the three refused pairs, evidence for the recorded function) is the right reading of the guide"
    requirement: D-15
    verification: []
    human_judgment: true
    rationale: "A representation choice beyond D-15's literal list; the owner confirms it in the 13-10 hand-off before the partner builds its sheet"

duration: 8min
completed: 2026-10-28
status: complete
---

# Phase 13 Plan 02: The motivation-enrichment-v1 Contract Summary

**`contracts/motivation-enrichment-v1.yaml` (kind schema 1.0.0) declares the per-post record `motivation-record-v1` and the expert's enrichment file `motivation-enrichment-v1` before any file exists. Every bound is structured: 10 compiled patterns, integer caps, a 1e-5 tolerance and closed enums. It also carries the 12-cell D-15 mapping, the five merge rules in order and the 17 refusal names. `pv validate` passes with 0 errors, and every corpus gate (census, clean-checkout graph, ontology ratchet, ONT-7, README count) is green.**

## Performance

- **Duration:** about 8 min
- **Started:** 2026-10-27T23:52:08Z
- **Completed:** 2026-10-28T00:00:19Z
- **Tasks:** 2
- **Files modified:** 3 (1 created, 2 regenerated)

## Accomplishments

- The contract lists the served task: labels `[social_affiliative, conspiratorial, other]` in served order, which the verify checks against `data/decide/motivation-v1/task.json` criteria. It pins task sha `f74a3ad3...` and artifact sha `d202f495...` in full.
- `motivation-record-v1` has exactly these 10 keys: post_id, source, text, labels, probabilities, artifact_sha256, deployment {pmcp_target, aws_account, region, endpoint}, tokens, truncated and scored_at. Each key has a type and structured rules, and the file states that records live only in the partner repository.
- `motivation-enrichment-v1` has a closed header (format const, annotation_mode enum with this_round prediction_shown, artifact_sha256, guide) and a closed row (post_id, text, label enum = served labels, evidence enum `[clear, weak, mixed, none]`, selection_slice enum). The caps are `{file_bytes 16777216, rows 20000, text_chars 20000}` and post_id must be unique.
- The `d15_amendment`, `mapping` (12 cells, quoting the guide's three Other-class sentences), `merge` (base-build-unchanged, dropped_eval_overlap, dropped_partner_mixed_group, dropped_conflict_groups, collapsed_same_label; train only; eval `ee95d71f...` byte-identical), `manifest` (`motivation-enrichment-manifest-v1`, counts and sha256 only), 17 `refusals`, `no_text_rule` and `anchoring` blocks are all declared.

## Gate results (recorded as the plan's output asks)

| Check | Result |
|-------|--------|
| `pv validate contracts/motivation-enrichment-v1.yaml` | `0 error(s), 0 warning(s)` / `Contract is valid.` |
| `pv lint contracts/motivation-enrichment-v1.yaml` | `Summary: 0 errors, 0 warnings, 0 suppressed`; `armed meet: Pass (8 armed; not armed: reverse-coverage, sigma, relations, shapes, valid-under)`; `Result: PASS` |
| Corpus-wide `pv lint contracts/` (OBSERVATION) | **rc 1**. `Summary: 2 errors, 1898 warnings, 0 suppressed, 2 new`; `armed meet: Fail (11 armed; not armed: reverse-coverage, valid-under)`. The armed gates are validate, audit, score, verify, enforce, enforcement-level, duplicate-stems, composition, sigma, relations and shapes. Both errors are sigma `PV-ONT-003` (`proof_obligations[2/3].formal` carries the character U+2717) in `contracts/llm-endpoint-openai-conformance-v1.yaml`, a Phase 9 contract this plan does not touch (already noted in 12-02-SUMMARY). No line names motivation-enrichment-v1. The valid-under gate shows 1852 contracts with 0 errors |
| valid_under | **kept**: `{world: committed}`; ONT-7 `ont7_valid_under_gate` 13 passed, 0 failed |
| Census delta | n_files 1851 to 1852, n_parsed 1851 to 1852, schema 768 to 769, unanchored 1843 to 1844, new id_set_sha256 `13678a9a...`; nothing else changed |
| Graph staleness before this plan | **not stale**: at 50a3f58bb, a fresh census matched `contracts/census.json` and `pv extract contracts --check` exited 0 in a clean worktree |
| Graph after | regenerated in a clean worktree. `contracts.nt` gained exactly 5 triples for `motivation-enrichment-v1` (17335 triples). `shapes.ttl` is byte-unchanged. `extract --check` exits 0 in a clean worktree of HEAD |
| `scripts/check_ont_ratchet.sh` | `PASS`, and `contracts/lint-baseline.json` is unchanged |
| `readme_contract` | 16 passed, 0 failed (README not edited; FALSIFY-README-007 allows lag) |

## Task Commits

1. **Task 1 (tracer): declare the contract**: `dc1dcb13d` (feat). The tracer gate ran in end-of-phase mode with an automated-only verify, so the verify was re-run and passed before Task 2 started.
2. **Task 2: corpus hygiene**: `3c055ebd4` (chore, census), `44c802893` (chore, contracts.nt from a clean checkout) and `77fcf488f` (fix, the `filename` key; see the deviation below).

## Files Created/Modified

- `contracts/motivation-enrichment-v1.yaml`: the contract (created)
- `contracts/census.json`: regenerated by `pv census contracts --format json`
- `contracts/contracts.nt`: regenerated by `pv extract contracts` in a clean `git worktree` of `3c055ebd4`

## Decisions Made

See the `key-decisions` frontmatter. The **d15_amendment is a decision for the owner**:
- the evidence enum adds the level `none`, for an other post with no evidence;
- the pairs other+clear, social_affiliative+none and conspiratorial+none are refused;
- `evidence` means the evidence for the recorded function.

It goes to the owner in the 13-10 hand-off, before the partner builds its sheet. The D-15 mapping itself (weak and mixed count as other) is unchanged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `manifest.file` raised the ontology ratchet's unanchored_but_bindable counter**
- **Found during:** Task 2 (`scripts/check_ont_ratchet.sh` printed `FAIL  unanchored_but_bindable ROSE 297 -> 298`)
- **Issue:** ONT R-5's counter (`count_unanchored_bindable`) counts any contract that has a line matching `^\s*(file|path|source_file):`. The plan's `manifest: file enrichment-manifest.json` produced such a line. The manifest names an output file the ingest writes, not a repo file to anchor.
- **Fix:** As the plan requires, the CONTRACT wording changed and the baseline did not. The key is now `manifest.filename`, with a comment explaining why. No downstream plan (13-03 to 13-10) reads `manifest.file`.
- **Files modified:** contracts/motivation-enrichment-v1.yaml
- **Verification:** the ratchet prints PASS with the baseline unchanged. The Task 1 verify still passes, the census is still current, and the full Task 2 verify exits 0.
- **Commit:** `77fcf488f`

**2. [Additive, within the plan's discretion] Structured additions beyond the action's literal list**
- `text.min_chars_after_trim: true` (the "after trim" qualifier as a value), `probabilities.items: number`, an optional `note` on most fields, `manifest.location`, and `d15_amendment.added_evidence_level_source`, `evidence_meaning_source` and `unchanged`. The mapping is 12 explicit cells, and `merge` also carries `base_train_sha256` and `identity: nfc-trim-ws-v1`. Every key set the verify pins (record fields, header, row, caps, refusals) is exactly as declared.

**Total deviations:** 1 auto-fixed (Rule 1) plus additive structure. **Impact:** downstream mirrors read `manifest.filename`, not `manifest.file`. Nothing else differs from the plan.

## Issues Encountered

- The corpus-wide `pv lint contracts/` exits 1 because of two sigma errors that were already there before this plan, in `contracts/llm-endpoint-openai-conformance-v1.yaml`. The plan treats this as an observation; they are out of scope and already noted in 12-02-SUMMARY.
- STATE/ROADMAP verbs, following the 13-01 convention:
  - `roadmap.update-plan-progress 13` ran (rc 0) and recorded 3/11 plans executed, with 13-02 checked.
  - The STATE.md verbs were deliberately NOT run. STATE.md carries the owner's uncommitted edit, which the dispatch says not to touch, and those verbs would write into it. Its bytes were confirmed unchanged by `cmp`.
  - `requirements.mark-complete` was skipped: the D-ids are CONTEXT decision ids, not REQUIREMENTS.md ids.
  - The owner's `.planning/STATE.md`, `config.json` and `state.json` were never staged.

## Next Phase Readiness

- Plans 13-03 (ingest) and 13-05 (record selector) can load this contract with pyyaml and check their constants against `formats.*`, `mapping.cells`, `merge.rules[].name`, `refusals[].name` and `caps`.
- Plan 13-10 must put the `d15_amendment` to the owner.

## Self-Check: PASSED

- FOUND: contracts/motivation-enrichment-v1.yaml, contracts/census.json, contracts/contracts.nt
- FOUND commits on this branch: dc1dcb13d, 3c055ebd4, 44c802893, 77fcf488f (4 commits measured from the ledger 50a3f58bb..77fcf488f)
- Task 1 and Task 2 `<automated>` verifies both exit 0 at 77fcf488f
