---
phase: 13
review: 13-REVIEW.md
titles: json
findings:
  - id: CR-01
    severity: critical
    disposition: deferred
    title: "An auth-on redeploy has no guard: it either contains a healthy server or silently reopens it (BLOCKER)"
  - id: WR-01
    severity: warning
    disposition: open
    title: "The token is minted at the start of the run and is visible to the whole build and deploy toolchain (WARNING)"
  - id: WR-02
    severity: warning
    disposition: open
    title: "The in-repo guards can be bypassed by a case-variant path on macOS (WARNING)"
  - id: WR-03
    severity: warning
    disposition: open
    title: "The D-06 triple ignores credentials set in environment variables (WARNING)"
  - id: WR-04
    severity: warning
    disposition: open
    title: "The probe-argv WIRING guard cannot see multi-line probe invocations (WARNING)"
  - id: WR-05
    severity: warning
    disposition: open
    title: "The \"offline\" auth selftest reads the developer's real pmcp config (WARNING)"
  - id: WR-06
    severity: warning
    disposition: open
    title: "Under `DRY_RUN=1`, `laya-probe-auth` still mints and probes a real endpoint, without the account check (WARNING)"
  - id: WR-07
    severity: warning
    disposition: open
    title: "The enrichment manifest drops the header's provenance (the model shown and the guide version) (WARNING)"
  - id: WR-08
    severity: warning
    disposition: open
    title: "The ingest's `--out` is not limited to the gitignored data tree (WARNING)"
  - id: WR-09
    severity: warning
    disposition: open
    title: "An OS error prints a traceback that includes the partner file's path (WARNING)"
  - id: WR-10
    severity: warning
    disposition: fixed
    title: "The new contract leaves the README contract count two behind (WARNING)"
  - id: IN-01
    severity: info
    disposition: open
    title: "`LAYA_M2M_MINTED` is a self-asserted flag that switches off the pre-set-token refusal (WARNING, low)"
  - id: IN-02
    severity: info
    disposition: open
    title: "The new re-exec line pastes `{{run}}`, `{{data}}` and `{{base}}` into shell text (WARNING, low)"
  - id: IN-03
    severity: info
    disposition: open
    title: "Helper rule naming and port leniency (WARNING, low)"
  - id: IN-04
    severity: info
    disposition: open
    title: "Record files are read with no size cap (WARNING, low)"
  - id: IN-05
    severity: info
    disposition: open
    title: "The contract's `deployment.this_round` triple is declared but not enforced (WARNING, low)"
  - id: IN-06
    severity: info
    disposition: open
    title: "\"Outside the checkout\" is not enforced for `LAYA_M2M_FILE` (WARNING, low)"
  - id: IN-07
    severity: info
    disposition: open
    title: "The default region conflicts with the second profile, and the snapshot covers only one extra profile (WARNING, low)"
open: 17
total: 18
recorded: 2026-10-28T08:39:05.874Z
---

# Phase 13: Code Review Disposition

| Finding | Severity | Disposition | Source |
|---------|----------|-------------|--------|
| CR-01 | critical | deferred | Owner, 2026-10-28 (UAT test 1): accepted control is the manual procedure in section 9 of 13-HANDOFF.md; nobody runs laya-deploy against augury-dev until the guard exists |
| WR-01 | warning | open | - |
| WR-02 | warning | open | - |
| WR-03 | warning | open | - |
| WR-04 | warning | open | - |
| WR-05 | warning | open | - |
| WR-06 | warning | open | - |
| WR-07 | warning | open | - |
| WR-08 | warning | open | - |
| WR-09 | warning | open | - |
| WR-10 | warning | fixed | 8bad5bae7: make readme-sync, 1852; check_readme_claims and readme_contract pass |
| IN-01 | info | open | - |
| IN-02 | info | open | - |
| IN-03 | info | open | - |
| IN-04 | info | open | - |
| IN-05 | info | open | - |
| IN-06 | info | open | - |
| IN-07 | info | open | - |

Dispositions: `open` (recorded, not yet triaged), `fixed`, `skipped`, `deferred`.
Set `deferred` by hand and put the reason in the Source cell; both are preserved. A `|` in the reason is kept as prose and escaped on the next run.
Re-running the gate keeps every row it can. A row the current review no longer reports is kept and its Source cell flagged, so a finding does not leave this record silently. ONE exception: when a finding id is REUSED by a different finding, the earlier decision cannot keep a row — the id is taken — and it is dropped. A RECORDED decision (anything but `open`) is named on the console when that happens; a row still at `open` is replaced silently, because `open` records no decision to lose.
