---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 11
subsystem: api
tags: [laya, mcp, decide, contract, door, classify_max_texts]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-01: 10-TASK-EVIDENCE.json (chosen_max_texts 8, measured.P 86, door_choice keep); plan 10-02: the lambda probe example"
provides:
  - "10-DOOR-EVIDENCE.json (laya-door-evidence-v1): the recorded no-op, the four mirrors at 8, the gate results and the probe checks"
  - "proof, measured at 024565d19, that the compiled constant, the contract constant, ledger row 75 and tests::bounds_match_contract agree on 8 and that 8 x 86 = 688 <= 800"
affects: [10-04, 10-07, 10-08, 10-10]

actuals:
  tokens: 1500
  tasks: 2
  commits: 2
plan_head_before: 024565d196baddefa17335df600f777b158b3821
plan_head_after: ae78e8e18203689d4365681c9486443de146d12f

tech-stack:
  added: []
  patterns:
    - "a door edit plan whose no-change branch is a measured, recorded no-op rather than a skipped step"

key-files:
  created:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-DOOR-EVIDENCE.json
  modified: []

key-decisions:
  - "The door stays 8: B = chosen_max_texts = 8 per the owner's reply 'door-8 p90' (2026-10-21), so the plan took its no-op branch and no mirror, contract version, ledger row, README, template comment or CLAUDE.md sentence changed"

requirements-completed: [SC-5, D-05, D-15]

coverage:
  - id: D1
    description: "Task 1: the four door mirrors (lib.rs CONTRACTED.max_texts, contract classify_max_texts, ledger row 75, tests::bounds_match_contract) agree on chosen_max_texts = 8 and 8 x P (86) = 688 <= 800; recorded as a no-op in 10-DOOR-EVIDENCE.json"
    requirement: "D-15"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-decide --lib: 32 passed, including tests::bounds_match_contract, tests::max_texts_fit_the_budget_at_the_shortest_row, tests::one_over_max_texts_refused_naming_max_texts"
        status: pass
      - kind: other
        ref: "python verify block against 10-TASK-EVIDENCE.json and 10-DOOR-EVIDENCE.json: DOOR MIRRORS OK B 8 P 86 action no-op contract 9.0.0"
        status: pass
    human_judgment: false
  - id: D2
    description: "The probe's replay range and plan-only door follow the compiled constant: --replay 9 exits 2 naming 1..=8; plan-only on the stance artifact prints texts == max_texts == 8, min_row_tokens 57"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "target/release/examples/probe --apr models/decide/laya-stance-64.apr --replay 9 (rc 2) and --plan-only --maximal distributed"
        status: pass
    human_judgment: false
  - id: D3
    description: "Task 2 skip recorded: docs_edited is empty and CLAUDE.md keeps its single 'at most 8 texts and 800 built tokens per call'"
    requirement: "D-05"
    verification:
      - kind: other
        ref: "Task 2 no-op branch of the verify block: DOOR DOCS: no-op recorded, nothing to edit; grep -c on CLAUDE.md = 1"
        status: pass
    human_judgment: false

duration: 15min
completed: 2026-10-21
status: complete
---

# Phase 10 Plan 11: Served door Summary

**B = 8, action no-op: the owner kept the door (`door-8 p90`), so the four mirrors were measured to agree on 8 with 8 x 86 = 688 <= 800 and nothing was edited; the contract stays at 9.0.0.**

## Result

- **B:** 8 (`10-TASK-EVIDENCE.json` `chosen_max_texts`, `previous_max_texts` 8, `door_choice` "keep"); **P:** 86; **B x P:** 688 against the 800 budget.
- **Action:** no-op. No lowering, so there is no `pv diff` suggestion (`pv_diff_suggestion: null`), and `metadata.version` stays `9.0.0` before and after.
- **Four mirrors, all 8:** `ClassifyLimits::CONTRACTED.max_texts` (lib.rs line 126), `classify_max_texts` (contract line 289), ledger row 75 anchor `classify_max_texts: 8`, and the mirror test `tests::bounds_match_contract` (ok). Ledger row 112 (`at most 8 texts and 800 built tokens per call`) also still holds.
- `served_task_min_row_tokens` is still 57 and the lambda README is untouched; plan 10-10 owns both.

## Tests and gates (each run at HEAD 024565d19, no source edit between runs)

| Gate | Result | How measured |
|------|--------|--------------|
| `cargo test -p aprender-mcp-decide --lib` | 32 passed, 0 failed; the three named tests ok | log redirected to a file, rc captured directly, `test result:` line and per-test lines read from it |
| `cargo test -p aprender-mcp-decide --test e2e_stdio` | 4 passed, 0 failed | same |
| `cargo test -p aprender-mcp-decide-lambda --lib` | 56 passed, 0 failed | same |
| `cargo clippy -p aprender-mcp-decide --all-targets --no-deps -- -D warnings` | rc 0 | log + rc |
| `cargo fmt -p aprender-mcp-decide -- --check` | rc 0 | rc |
| `pv validate contracts/decide-tool-boundary-v1.yaml` | `0 error(s), 0 warning(s)`, "Contract is valid." | `cargo run -p aprender-contracts-cli --bin pv`, rc 0 |
| `just laya-claims-check` | `LAYA CLAIMS OK 133 rows` | rc 0 |
| `make contract-audit-phase8` | rc 0, "no Phase 8 binding finding" | rc 0 |
| probe `--replay 9` (B+1) | exit 2, `probe: --replay must be in 1..=8 (the compiled classify_max_texts)` | binary built from the current tree (`cargo build --release --example probe`) |
| probe `--plan-only --maximal distributed` (stance artifact) | `texts` 8, `max_texts` 8, `min_row_tokens` 57, `tokens_planned` 800 | JSON line read from the log |

All cargo and make commands ran as `CARGO_INCREMENTAL=0 rtk proxy ...` per the plan's executor note, with output redirected to files and `$?` read directly (never through a pipe).

## Task 2

Skipped on the record, as the plan prescribes at `no-op`: the decide README, the lambda deploy-template comment, CLAUDE.md and ledger row 112 keep saying 8 and remain true. `docs_edited` is `[]` and `task2_skip` explains why in the evidence file; CLAUDE.md still carries exactly one `at most 8 texts and 800 built tokens per call`.

## Host courtesy

I did not wait. `/tmp/p10-03-probe.running` was absent at every check (plan 10-03 had already finished), so 0 minutes of the 60-minute allowance were used.

## Deviations from Plan

None - plan executed exactly as written, on its keep-8 branch. `historical_residue_unchanged` is `[]` as the no-op branch requires. `10-DOOR-EVIDENCE.json` `commit` field says "no production commit (no-op)" because the evidence file cannot name the commit that contains it; the commits are listed below.

### State handling (disclosed hand edit)

The state verbs (`advance-plan`, `record-metric`, `add-decision`, `record-session`) refuse in this project because `04-VERIFICATION.md` has status `gaps_acknowledged`, and I did not re-key that report. `roadmap update-plan-progress` was used; STATE.md was hand-edited minimally (the Last activity line only). See the final commit.

## Known Stubs

None. No source file was created or modified.

## Threat Flags

None. No file with a security surface was touched. T-10-37 (a one-sided mirror edit) is mitigated by construction here: nothing was edited, and the verify extracted each mirror's value and compared it to `chosen_max_texts`. T-10-48 (a lowered bound committed before the gate) does not arise at B = 8.

## Commits

- `a8734c53c` feat(10-11): record the door as a no-op, chosen_max_texts 8 agrees across the four mirrors
- `ae78e8e18` docs(10-11): record the Task 2 skip, no door document edited at B = 8

`commits: 2` is measured from the persisted ledger (`git rev-list --count 024565d19..HEAD` before this SUMMARY).

## Self-Check: PASSED

- FOUND: .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-DOOR-EVIDENCE.json
- FOUND commits: a8734c53c, ae78e8e18
