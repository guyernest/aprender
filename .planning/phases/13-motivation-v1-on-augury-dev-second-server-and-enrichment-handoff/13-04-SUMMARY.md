---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 04
subsystem: decide-deploy-tooling
status: complete
tags: [justfile, deployment-triple, pmcp-run, aws-account, state-dir, gates, mutation-testing, offline-selftest]
requires:
  - phase: 13-11
    provides: "D-06 baseline: the ze-kasher-dev archive and its pinned MANIFEST.sha256"
  - phase: 13-01
    provides: "the region parameter, _laya-state-snapshot, the eligibility export-before-restore, R3/R7 in _laya-region-selftest"
provides:
  - "_laya-target-check (committed profile -> target/region/host suffix/account table, env and pmcp-config agreement)"
  - "_laya-account-check (live sts against the table's account; no call under DRY_RUN=1 unless LAYA_ACCOUNT_CHECK_FORCE=1)"
  - "_laya-state-dir, _laya-endpoint-check, _laya-pmcp-fixture-config, _laya-target-selftest"
  - "seven AWS-touching recipes wired in order (target check < account check < first AWS line), exporting the checked target"
  - "gate rows target-check, account-check, state-dir, endpoint-check, target-selftest"
  - "13-TARGET-EVIDENCE.json (laya-target-evidence-v1)"
affects: [13-05, 13-06, 13-07, 13-08, 13-09, 13-10]
actuals:
  tokens: 24416
  tasks: 3
  commits: 4
commits: 4
plan_head_before: a592b35cf7868f45cc7022f88bd0ae6cc75048d4
plan_head_after: 7418298bc5edb1da52e50101371fbaa529b1e00b
tech-stack:
  added: []
  patterns:
    - "the deployment triple, not the server id, keys local state: models/decide/<profile>/ for any profile but ze-kasher-dev"
    - "check, then export the checked PMCP_TARGET / AWS_PROFILE / AWS_REGION and unset PMCP_API_URL / AWS_DEFAULT_REGION, so cargo-pmcp injects the target's own api_url"
    - "a wiring guard that compares LINE INDICES in `just --dump` bodies, not mere presence"
key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-TARGET-EVIDENCE.json
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/deferred-items.md
  modified:
    - justfile
    - scripts/laya_gates.tsv
key-decisions:
  - "_laya-endpoint-check reads the host suffix from _laya-target-check, so the target table exists in exactly one place"
  - "laya-deploy and laya-upload run the live account check AFTER eligibility (5b), keeping 'nothing reaches AWS before the verifier accepted'; the target check (pure) runs before it"
  - "the teardown refusal is kept (a containment on the wrong tenant is harmful); reachability from contain() is proven by TEARDOWN REACHABLE, not by softening it"
  - "a small private helper _laya-pmcp-fixture-config writes the one fixture config the four offline selftests and the gates rows share"
requirements-completed: [D-06, D-02, D-19]
coverage:
  - id: D1
    description: "Target-explicit augury deploy path (laya-deploy-config, laya-deploy) with ISOLATION 1-3 and the planted-augury restore"
    requirement: D-06
    verification:
      - kind: other
        ref: "Task 1 <automated>: just _laya-target-selftest; AUG block (AUGURY RESTORE OK); live-config target check; stale PMCP_TARGET / PMCP_API_URL refused"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every AWS-touching recipe ordered and target/account-explicit; TARGET/ACCOUNT/STATE/ENDPOINT/WIRING/TEARDOWN REACHABLE/VERIFY STATE tables; five gate rows"
    requirement: D-06
    verification:
      - kind: other
        ref: "Task 2 <automated>: target selftest under stale ambient variables; LAYA_GATES_ONLY sweep of 13 rows (DRIFT CHECK OK, LAYA GATES ROWS OK); motivation-id deploy selftest; snapshot cmp; 13-11 manifest and pin"
        status: pass
    human_judgment: false
  - id: D3
    description: "ze-kasher-dev record unreachable from augury-dev commands, and the default render byte-identical to the pre-phase justfile's"
    requirement: D-02
    verification:
      - kind: other
        ref: "13-TARGET-EVIDENCE.json default_profile_equivalence (cmp equal, both nested helpers identical); ISOLATION 1/2"
        status: pass
    human_judgment: false
  - id: D4
    description: "K1..K10 and R-AUG re-mutation, plus 13-TARGET-EVIDENCE.json"
    requirement: D-19
    verification:
      - kind: other
        ref: "Task 3 <automated>: TARGET EVIDENCE OK"
        status: pass
    human_judgment: false
duration: 62 min
completed: 2026-10-28
---

# Phase 13 Plan 04: The Deployment Triple as the Key Summary

**Every laya recipe that touches AWS or pmcp.run now names its deployment as (pmcp.run target, AWS account, region). Each one checks that triple against a committed table and the pmcp config, then checks the account live with sts, all before its first AWS line. It then exports the checked target, so cargo-pmcp and the AWS CLI cannot follow a stale shell variable. augury-dev keeps its local state under `models/decide/augury-dev/`, so an augury publish can no longer overwrite the ze-kasher-dev record. Each guard was mutated, and each mutation turned its named case RED.**

## Performance

- **Duration:** 62 min (2026-10-28T00:20Z to 01:22Z)
- **Tasks:** 3 (tracer, TDD auto, auto)
- **Files modified:** 3 committed by the tasks, plus deferred-items.md

## Accomplishments

- **The committed table** in `_laya-target-check`:

  | profile | target | region | host suffix | account |
  |---|---|---|---|---|
  | ze-kasher-dev | dev | us-east-1 | .us-east.true-mcp.com | 816069129666 |
  | augury-dev | augury-dev | eu-west-1 | .eu-west.augury.pmcp.run | 783960389328 |

  - A profile outside the table is refused, and so is a region that disagrees with it.
  - The same goes for any set PMCP_TARGET, AWS_PROFILE, AWS_REGION, AWS_DEFAULT_REGION or PMCP_API_URL that points elsewhere.
  - The pmcp config is cross-checked: type pmcp-run, aws_profile, region, and an execute-api api_url in that region.
  - The real `~/.pmcp/config.toml` gives `augury-dev eu-west-1 .eu-west.augury.pmcp.run 783960389328`.
- **`_laya-account-check`** runs a live sts call against the table's account. Under DRY_RUN it makes no call unless forced.
- **`_laya-state-dir`**: `models/decide` for ze-kasher-dev, so every existing path is unchanged; `models/decide/<profile>` for any other profile.
- **`_laya-endpoint-check`**: the endpoint must be `https://<server><suffix>/mcp` with an anchored suffix. Loopback is accepted only under DRY_RUN=1.
- **Seven recipes wired in order:** laya-deploy-config, laya-deploy, laya-upload, laya-grant, laya-deploy-verify, laya-teardown and laya-weights-bucket.
  - laya-deploy now contains a deployed endpoint that is not on its tenant. It prints `TARGET RECEIPT:` from cargo-pmcp's banner, and contain()'s fallback names `--profile` and `--region`.
  - laya-deploy-verify checks STATE and the endpoint before the probe build.
- **Five gate rows** were added, and the four offline selftests unset the ambient target variables at setup.

### Quoted lines (from the final restored run and the Task 3 evidence)

```
ISOLATION 1 render: augury-dev wrote models/decide/augury-dev/deploy-config-aprender-mcp-decide-antisemite.toml; the default models/decide/deploy-config-aprender-mcp-decide-antisemite.toml sha256 unchanged (4e0f56db6342)
ISOLATION 2 no-augury-config: REFUSED config naming models/decide/augury-dev/deploy-config-aprender-mcp-decide-antisemite.toml although models/decide/deploy-config-aprender-mcp-decide-antisemite.toml exists (exit 2)
ISOLATION 3 augury-config: reached REFUSED eligibility: REFUSED SyntheticNotDeployable after target-check ok on models/decide/augury-dev/deploy-config-aprender-mcp-decide-antisemite.toml
WIRING order laya-deploy: target 49 < account 80 < aws 96
WIRING order laya-deploy-config: target 38 < account 40 < aws 50
WIRING order laya-deploy-verify: target 16 < account 18 < aws 63
WIRING order laya-grant: target 14 < account 16 < aws 19
WIRING order laya-teardown: target 14 < account 16 < aws 20
WIRING order laya-upload: target 20 < account 49 < aws 54
WIRING order laya-weights-bucket: target 17 < account 19 < aws 22
TEARDOWN REACHABLE accept: PMCP_TARGET=augury-dev AWS_PROFILE=augury-dev AWS_REGION=eu-west-1 (laya-deploy's exports) -> CONTAINED: aprender-mcp-decide-antisemite reserved concurrency 0 (no invocation runs, warm, target and account checks passed
TEARDOWN REACHABLE refuse: the twin with AWS_REGION=us-east-1 is REFUSED target (exit 2), no put-function-concurrency call
AWS CALLS: 0
TARGET SELFTEST OK
AUGURY RESTORE OK
```

The `TEARDOWN REACHABLE accept` line is cut short by the selftest's own `cut -c 1-90` on the CONTAINED text; the case's checks are not truncated. The selftest has 13 TARGET must-refuse cases and 4 must-pass cases, plus the ACCOUNT, STATE, ENDPOINT and VERIFY STATE tables: 58 case lines in all.

### Mutation table (each run of `_laya-target-selftest` RED on its case, each restore GREEN)

| id | edit | RED line (abridged) | restored |
|---|---|---|---|
| K1 | laya-deploy loses its target check call | `FAIL WIRING WIRING order laya-deploy: target None, account 79, first aws 95` | TARGET SELFTEST OK |
| K2 | `_laya-state-dir` prints models/decide for every profile | `FAIL STATE accept 'augury-dev': exit 0, got 'models/decide'` (also ISOLATION 1/2, VERIFY STATE) | TARGET SELFTEST OK |
| K3 | target check drops the AWS_REGION agreement | `FAIL TARGET refuse augury-dev with AWS_REGION=us-east-1: exit 0` (also the teardown twin) | TARGET SELFTEST OK |
| K4 | endpoint check stops comparing the suffix | `FAIL ENDPOINT refuse https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp for augury-dev` (also the spoof hosts) | TARGET SELFTEST OK |
| K5 | deploy-verify STATE back to models/decide | `FAIL VERIFY STATE augury-dev: exit 0, expected <endpoint from models/decide/augury-dev/...>` | TARGET SELFTEST OK |
| K6 | laya-deploy loses `export PMCP_TARGET` | `FAIL WIRING WIRING laya-deploy export: export PMCP_TARGET None` | TARGET SELFTEST OK |
| K7 | laya-upload's target check moved after its first aws line | `FAIL WIRING WIRING order laya-upload: target 54, account 48, first aws 53` | TARGET SELFTEST OK |
| K8 | target check drops the PMCP_API_URL agreement | `FAIL TARGET refuse augury-dev with PMCP_API_URL = the us-east-1 control plane: exit 0` | TARGET SELFTEST OK |
| K9 | account check accepts any 12-digit account | `FAIL ACCOUNT refuse augury-dev, sts answers ze-kasher-dev's account: exit 0` | TARGET SELFTEST OK |
| K10 | laya-teardown passes the literal us-east-1 | `FAIL TEARDOWN REACHABLE accept: exit 2` | TARGET SELFTEST OK |
| R-AUG | region selftest drops the augury path from its backup/restore | `the region selftest did not restore the models/decide/augury-dev/ config path` | AUGURY RESTORE OK |

The default profile is unchanged. A DRY_RUN render of the pre-phase justfile (`bc52bf3d6`) cmp-equals the current render. The pre-phase recipe calls two helpers through nested `just`, which resolve to the current justfile; both (`_laya-server-check`, `_laya-iam-check`) are byte-identical in the pre-phase and current justfiles, so no worktree render was needed.

## Task Commits

1. **Task 1 (tracer): target-explicit augury deploy path.** `fb898762b` (feat). The tracer gate ran in end-of-phase mode with an automated-only verify, and the verify passed before expansion.
2. **Task 2 RED: the failing tables and five gate rows.** `b4bbf312c` (test)
3. **Task 2 GREEN: every AWS-touching recipe wired in order.** `993f87da8` (feat)
4. **Task 3: 13-TARGET-EVIDENCE.json.** `7418298bc` (docs)

## TDD Gate Compliance (Task 2)

- **RED (`b4bbf312c`):** `_laya-target-selftest` exited 1 with `TARGET SELFTEST FAILED (8 failed checks)`. Each failure was a planned assertion:
  - five `FAIL WIRING WIRING order <recipe>: target None, account None` lines, one each for laya-upload, laya-grant, laya-teardown, laya-weights-bucket and laya-deploy-verify;
  - the endpoint-before-build rule;
  - the TEARDOWN REACHABLE twin not refused (exit 0);
  - VERIFY STATE naming `models/decide/deploy-...state`.
- **RED assessment:** every case that the RED change did not target (ISOLATION, TARGET, ACCOUNT, STATE, ENDPOINT) passed, and the shell ran without error. The failure is the intended missing behaviour, not a fixture or harness fault.
- **Why no classifier record:** these are justfile selftests whose output is not in a format the `tdd-red-evidence` classifier supports (TAP, JUnit, go, swift-testing, unittest). So no classifier record was produced, and this text assessment stands in for it, as in 13-01.
- **GREEN (`993f87da8`):** `TARGET SELFTEST OK` under stale ambient variables, and the full Task 2 verify had rc 0.
- **REFACTOR:** none.

## Decisions Made

See `key-decisions` in the frontmatter. One placement detail: in laya-deploy, the pure target check runs right after the pin checks, at step (3c), and the live account check runs after eligibility, at step (5b). Both lines still come before the first AWS line, which the WIRING guard confirms.

## Deviations from Plan

**1. [Rule 1 - Side effect, restored] The filtered gates sweep rewrote the antisemite id's two eligibility logs.**
- **Found during:** a plan-level snapshot check against the plan-start baseline, after Task 2.
- **Issue:** the rows deploy-refusals, upload-eligibility and sha256-helper call laya-deploy and laya-upload directly. That rewrites `models/decide/eligibility{,-upload}-aprender-mcp-decide-antisemite.log`, and the gates selftest's cleanup does not restore them. This predates this plan. The Task 2 verify takes its own snapshot after the sweep, so it could not see the change.
  - Before and after, both files were tiny-fixture `SyntheticNotDeployable` logs. Only the build time and the fixture path differed.
  - No live record is involved: the 13-11 manifest, which covers the motivation id, verified throughout.
- **Fix:** the plan-start bytes were restored from a backup taken before the plan started (`cp -p`), and `_laya-state-snapshot` then matched the plan-start snapshot. Fixing the gates selftest itself is out of scope, so it is logged in `deferred-items.md`.

**2. [Rule 2 - Shared fixture] One small private helper was added.** `_laya-pmcp-fixture-config` writes the fixture pmcp config. The four offline selftests and the gates rows use it, instead of four inline copies. It is not a gate, and the drift check classifies it that way.

**3. [Rule 3 - Typo, fixed before commit]** An Edit dropped the space in `laya-deploy-config apr`. The first selftest run caught it ("Justfile does not contain recipe"), and it was fixed before the Task 1 commit.

**4. [Evidence hygiene]** The evidence builder replaces any 12-digit number other than the two table accounts with `<fake-account>`; none occurred.

**5. [Environment]** The verify commands' `/tmp/p13-04-*` logs were written to the session scratchpad instead. The commands were otherwise unchanged.

**Total deviations:** 5. **Impact:** none on the plan's goal. No ze-kasher-dev local file changed.

## Issues Encountered

None beyond the deviations above. No live AWS or pmcp.run call was made: every run reported `AWS CALLS: 0`, and the scripted fakes were placed first on PATH for one table each. `cargo pmcp configure use` was never run.

## Known Stubs

None.

## Threat Flags

None. T-13-10, -11, -12, -13, -41, -42 and -43 are mitigated as registered, and each is shown RED under its mutation (K1/K3, K2/K5, TARGET RECEIPT, K4, K8, K7/K9, K10).

## STATE / ROADMAP

- `roadmap.update-plan-progress 13`: its outcome is recorded in the docs commit that follows this summary.
- The STATE.md verbs were deliberately not run. STATE.md carries the owner's uncommitted edit, and those verbs would write into it. This follows 13-01.
- `requirements.mark-complete`: skipped, because D-06, D-02 and D-19 are CONTEXT decision ids, not REQUIREMENTS.md ids.

## Next Phase Readiness

Plans 13-07..13-09 can render, upload, deploy and verify on augury-dev with `profile augury-dev` and `region eu-west-1`. Their state lands in `models/decide/augury-dev/`. A stale `dev` target in the shell, or the repo-root `.pmcp/active-target`, can no longer redirect them.

## Self-Check: PASSED

- FOUND: justfile `_laya-target-check`, `_laya-account-check`, `_laya-state-dir`, `_laya-endpoint-check`, `_laya-target-selftest`; scripts/laya_gates.tsv `target-selftest`; 13-TARGET-EVIDENCE.json (`TARGET EVIDENCE OK`)
- FOUND commits: fb898762b, b4bbf312c, 993f87da8, 7418298bc (ancestors of HEAD); `git rev-list --count a592b35cf..HEAD` = 4 before this summary's commit
- `git diff --quiet -- justfile` holds; `shasum -a 256 -c` on the 13-11 manifest passes; `_laya-state-snapshot` equals the plan-start snapshot
