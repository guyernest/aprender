---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 04
subsystem: decide-server-deployment-prep
tags: [laya, decide, thin-crate, deployment-unit-register, edge-health, retry, justfile, offline-selftest]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "plan 12-01 (OWN-HUNKS COMMIT procedure for justfile)"
provides:
  - "crates/aprender-mcp-decide-motivation-lambda: publish = false shim, bin bootstrap, one dependency on aprender-mcp-decide-lambda"
  - "FALSIFY-MONO-011 deployment-unit register 9 -> 10 under Phase 12 D-02; duplicate-bin allowlist bootstrap line with the fifth claimant; README workspace crates 92 -> 93"
  - "models/decide/resolver-proof-aprender-mcp-decide-motivation.txt (gitignored) for cargo-pmcp 0.28.1; an arm64 bootstrap that names the crate"
  - "just _laya-edge-health-get url out (F-10-12-1): 404-only bounded retry of the first edge /health GET; laya-deploy wired to it"
  - "laya-deploy-selftest EDGE HEALTH get table (eight cases) plus an extended wiring check; laya_gates.tsv row edge-health-get with row_edge_health_get()"
affects: [12-12]

actuals:
  tokens: 7200
  tasks: 2
  commits: 2
plan_head_before: b5ec8f023da2f0da9c34312658303e74868ad899
plan_head_after: 4a6ff5ee4a2c33ee96d310d48f8e1c256c6818b8

tech-stack:
  added: []
  patterns:
    - "a retry bounded by elapsed wall time (not TRIES x SLEEP) because each attempt can itself spend the curl timeout"
    - "a scripted curl shim on PATH for one table only (restored after it), pre-seeding a stale healthy body to prove a refusal leaves none"

key-files:
  created:
    - crates/aprender-mcp-decide-motivation-lambda/Cargo.toml
    - crates/aprender-mcp-decide-motivation-lambda/src/main.rs
    - crates/aprender-mcp-decide-motivation-lambda/README.md
  modified:
    - Cargo.toml
    - Cargo.lock
    - README.md
    - crates/aprender-core/tests/monorepo_invariants.rs
    - scripts/duplicate_bin_names_allowlist.txt
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "Register read from HEAD: DEPLOYMENT_UNIT_BASELINE was 9 (Phase 11 plan 11-06 had not landed), so it is now 10 with a history line citing Phase 12 D-02"
  - "The helper refuses a bad retry configuration with exit 2 and also removes the out file, so even a config refusal cannot leave a stale deploy-health file"
  - "A 404 that hits TRIES is reported as (tries N); one that hits the elapsed cap as (elapsed cap M s); TRIES is checked first"
  - "laya-deploy also contains if the kept body file cannot be read back, instead of exiting under set -e without containment"

requirements-completed: [D-02, D-23]

duration: about 50 min
completed: 2026-10-26
---

# Phase 12 Plan 04: Motivation server crate and the bounded first-health retry Summary

**Thin `aprender-mcp-decide-motivation-lambda` crate admitted by every register under the owner's D-02 growth (9 -> 10), arm64 bootstrap and resolver proof built offline, and `laya-deploy`'s first edge /health GET retries HTTP 404 only within a 900 s elapsed cap, proven by an eight-case offline table with zero AWS calls.**

## Task 1 (tracer): the new server id end to end offline

Commit `932b58465`.

- Crate copied from the antisemite template: `publish = false`, `[[bin]] bootstrap`, the one path dependency, `[lints] workspace = true`; `main.rs` is the three-line shim calling `aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME"))`; README states the shim, "one server id, one thin crate", the zigbuild command, the three pilot labels and the monorepo link.
- Register value before and after: `DEPLOYMENT_UNIT_BASELINE` **9 -> 10** (read from HEAD `b5ec8f023`; the register lists 10 entries), history line "9 -> 10: GROWN (Phase 12 D-02 ...)" added. Allowlist `bootstrap` line gained `aprender-mcp-decide-motivation-lambda` in sorted order (fifth claimant, comment updated). README workspace crates **92 -> 93**. Cargo.lock grew by 7 lines (one path crate, no registry crate); `cargo check -p aprender-mcp-decide-motivation-lambda --offline` clean.
- Resolver proof (`just laya-resolver-proof /Users/guy/Development/mcp/sdk/rust-mcp-sdk d0348451 /tmp/p12-04-proof aprender-mcp-decide-motivation`):
  - `server ok: aprender-mcp-decide-motivation (named)`
  - `RESOLVER PROOF: crates/aprender-mcp-decide-motivation-lambda (control: root crates/aprender-mcp-decide-motivation-lambda -> crates/aprender-mcp-chronos-lambda)`
  - `cargo-pmcp 0.28.1, source d03484517dce988355b7e9bf3dd2746eec188019 (builder.rs last changed in b65d354ff23c971b8353476ed7fcbde661b2ae41)`
  - proof file first line `crates/aprender-mcp-decide-motivation-lambda`, `cargo_pmcp_version=0.28.1`
- Bootstrap: `BOOTSTRAP aarch64 OK /Users/guy/Development/machine-learning/aprender/target/aarch64-unknown-linux-gnu/release/bootstrap (26101832 bytes)`.
- Verified: `monorepo_invariants` (15 passed), `check_duplicate_bin_names.sh` PASS, `thin_server_crates_are_shims` ok, `readme_contract` (16 passed, crate count and monorepo link ok), and the plan's register assertion script printed `REGISTER OK baseline 10 entries 10`. justfile was not staged by Task 1.

## Task 2: the 404-only bounded retry of the edge /health GET (F-10-12-1)

Commit `4a6ff5ee4`, committed through the OWN-HUNKS procedure (`/tmp/p12-04-justfile-*`).

- `_laya-edge-health-get url out`: TRIES (default 40), SLEEP (default 15), MAX seconds (default 900), all validated as whole numbers before any request (TRIES >= 1; exit 2, `REFUSED edge-health-get: config ...`); `curl -sS --max-time 30 -o out -w '%{http_code}'` with curl's own status captured (never `-f`); only 404 retries, and only while `k < TRIES` and `elapsed + SLEEP + 30 <= MAX`; every refusal removes the out file and the file is truncated before each attempt.
- Stated worst-case elapsed bound: **900 s (15 min) by default**; a retry is never started past the cap, whereas 40 x (30 s curl timeout + 15 s sleep) alone would allow about 29.75 min.
- `laya-deploy` now calls `just _laya-edge-health-get "$HEALTH_URL" "models/decide/deploy-health-$SERVER.json" || contain ...`, reads the body back (contained if unreadable) and keeps `just _laya-edge-health-check "$HEALTH" "$SERVER"` byte-identical.
- The eight health-GET case lines from `LAYA_SELFTEST_SERVER=aprender-mcp-decide-motivation LAYA_DEPLOY_SELFTEST_POSITIVE=0 just laya-deploy-selftest`:
  - `EDGE HEALTH get accept: ok-first (1 calls)`
  - `EDGE HEALTH get accept: 404-404-200 (3 calls)`
  - `EDGE HEALTH get refuse: 404-forever (3 calls)`
  - `EDGE HEALTH get refuse: 500 (1 calls)`
  - `EDGE HEALTH get refuse: 503 (1 calls)`
  - `EDGE HEALTH get refuse: transport (1 calls)` (stale healthy body pre-seeded; absent or empty afterwards)
  - `EDGE HEALTH get refuse: config (0 calls)` (four invalid settings)
  - `EDGE HEALTH get refuse: elapsed-cap (1 calls)`
  - plus `EDGE HEALTH wiring: laya-deploy GETs $HEALTH_URL through the retrying helper, checks its serverId and never GETs $ENDPOINT`
- Results: motivation id ended `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)` with `AWS CALLS: 0`; the antisemite id (no regression) ended the same with `AWS CALLS: 0`; `LAYA_GATES_ONLY=edge-health-get just laya-gates-selftest` printed `DRIFT CHECK OK ... 33 rows`, `ROW edge-health-get: GREEN`, `LAYA GATES ROWS OK 1 of 33`; the url, check and gates-selftest rows and `laya-claims-check` (140 rows) stay green.
- TDD: the table was written first and run RED against the missing helper (13 failed checks), then GREEN after the helper. A mutation run (500 retried, stale body not removed) was caught by the 500 and 503 cases and restored byte-identical.
- D-23: after the commit `git diff HEAD -- justfile` equals the owner's foreign diff line for line, and the committed `start..HEAD` range equals the own patch as a multiset (`HEALTH RETRY COMMITTED; committed range == own hunks; FOREIGN DIFF PRESERVED`). Nothing reached AWS or pmcp.run.

## Deviations from Plan

None - plan executed as written. One small hardening beyond the text: `laya-deploy` adds `|| contain` to the read-back of the kept body file (Rule 2), so an unreadable file cannot exit under `set -e` without containment.

## Known Stubs

None.

## Threat Flags

None. The new crate adds a deployment unit already covered by T-12-16 and no new network or auth surface.

## Tracking

`gsd_run query state.*` / roadmap updates were not run: per the dispatch note they fail on `04-VERIFICATION.md` status `gaps_acknowledged`, so STATE.md, ROADMAP.md and REQUIREMENTS.md are left to the orchestrator. No pre-existing uncommitted file (STATE.md, config.json, state.json, foreign justfile diff) was touched or staged.

## Self-Check: PASSED

- crates/aprender-mcp-decide-motivation-lambda/{Cargo.toml,src/main.rs,README.md}: present
- commit 932b58465 and commit 4a6ff5ee4: ancestors of HEAD
- models/decide/resolver-proof-aprender-mcp-decide-motivation.txt: present (gitignored)
