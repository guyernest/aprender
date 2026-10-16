---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 06
subsystem: infra
tags: [justfile, aws-cli, lambda, function-url, selftest, bashrs, fail-closed]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: "09-04: pin.env, llm-endpoint-image (.ids/image-local.json, image-pushed.json); 09-11: ladder_pin.py, buildspec-weights.yaml, iam/*.json, README runbook"
provides:
  - "just llm-endpoint-deploy: ten-rung fail-closed ladder with DRY_RUN (offline) and PREFLIGHT (connected read-only) modes, live create/update with a readback"
  - "just llm-endpoint-up / -weights / -prove-401 / -url / -warm / -teardown: the D-12 sequence as recipes, aws-cli only"
  - "just llm-endpoint-selftest: aws shim first on PATH, 41 cases, AWS CALLS: 0, FIXTURE READS counter, negative control"
  - "deploy/llm-endpoint/events/apigw-v2-no-auth.json and the README recipe/mode/teardown/cost sections"
affects: [09-07 go/no-go (runs PREFLIGHT), 09-13 measure (consumes .ids/url.json, function.json), 09-08 (T-09-50 disposition)]

actuals:
  tokens: 25200
  tasks: 2
  commits: 2
plan_head_before: 2bdca505b97703d2c1498e44fc3882989dcc4276
plan_head_after: 1b72513cd2724b2b3c70a6593c21599e0437800f

tech-stack:
  added: []
  patterns:
    - "aws shim on PATH in three modes (refuse / preflight / permissive) with a per-case argv log: tests assert which AWS calls were and were NOT made"
    - "tolerate <exact exception> <what> <cmd>: idempotency by exact exception name, never by swallowing the exit status"
    - "state handed between recipes through gitignored .ids/*.json files, each checked by the consumer (prove-401.json -> url, image-pushed.json -> deploy/teardown)"
    - "mutation probes of every guard against the case table (18 probes; 4 missed, each pinned by a new case)"

key-files:
  created:
    - deploy/llm-endpoint/events/apigw-v2-no-auth.json
  modified:
    - justfile
    - deploy/llm-endpoint/README.md

key-decisions:
  - "The ECR repository of an environment is never deleted by a recipe, not even under TEARDOWN_SPIKE_031=1: the spike-031 repository removal is printed like the environment one"
  - "An account guard (sts equals aws.json) was added to up, weights, url and teardown, beyond deploy rung 8: every recipe that writes or deletes checks it first"
  - "weights records .ids/weights.json (mode, bucket, key, CodeBuild project and role) so teardown removes the CodeBuild resources only when weights codebuild made them"

patterns-established:
  - "A guard added without a case is unproven: every guard here was mutated, and the four that survived got a case"

requirements-completed: [IR-4, IR-5, SC-3, SC-4, SC-5]

coverage:
  - id: D1
    description: "llm-endpoint-deploy ladder: ten rungs plus mode, DRY_RUN is offline with zero aws calls, PREFLIGHT runs only the three read calls, live path reads back memory/timeout/ephemeral/arch"
    requirement: "IR-4"
    verification:
      - kind: unit
        ref: "just llm-endpoint-selftest cases 1-13, 26-28 -> LLM-ENDPOINT SELFTEST OK 41/41, AWS CALLS: 0, FIXTURE READS: 9"
        status: pass
    human_judgment: false
  - id: D2
    description: "llm-endpoint-url creates no URL without a fresh 401 proof for the deployed digest and bounded concurrency, removes URL and both statements on a non-401"
    requirement: "SC-3"
    verification:
      - kind: unit
        ref: "selftest cases 16, 17, 20-24, 38 (shimmed aws and curl)"
        status: pass
    human_judgment: false
  - id: D3
    description: "llm-endpoint-teardown contains first, removes both permission statements, verifies get-policy, deletes only the recorded image tag, never executes delete-repository"
    requirement: "SC-4"
    verification:
      - kind: unit
        ref: "selftest cases 18, 19, 29-31, 37, 39-41; static verify: no non-echo delete-repository line"
        status: pass
    human_judgment: false
  - id: D4
    description: "every llm-endpoint recipe body is bash with set -euo pipefail, calls aws by its bare name, and lints with 0 bashrs errors"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "plan Task 2 verify loop (nine recipes incl. llm-endpoint-image) -> recipe bodies lint clean"
        status: pass
    human_judgment: false
  - id: D5
    description: "the recipes work against a real AWS account"
    requirement: "IR-4"
    verification: []
    human_judgment: true
    rationale: "Nothing here reaches AWS by design (the shim is the only aws). The first real run is the D-15 go/no-go in plan 09-07; flag syntax of aws-cli calls (e.g. --query shapes, --cli-input-json for retention) is verified against the shim, not the service."
  - id: D6
    description: "README describes the recipes, the mode contract, env-specific names, teardown, cost amplification and the URL as a secret-equivalent address"
    requirement: "IR-5"
    verification:
      - kind: other
        ref: "Task 2 verify greps (## Recipes (the D-12 order), cost amplification, secret-equivalent, PREFLIGHT); no 64-hex string, no .on.aws host"
        status: pass
    human_judgment: false

duration: ~2h (start time was not recorded at launch)
completed: 2026-10-15
status: complete
---

# Phase 9 Plan 06: Lifecycle recipes and the refusing deploy ladder Summary

**A ten-rung `llm-endpoint-deploy` with an offline DRY_RUN and a connected read-only PREFLIGHT, plus up / weights / prove-401 / url / warm / teardown recipes that encode D-12 (no URL before a fresh 401 for the deployed digest and bounded concurrency; teardown that can only delete the image tag it recorded), all proven by a 41-case self-test under an aws shim with `AWS CALLS: 0`.**

## Performance

- **Tasks:** 2 (Task 1 tracer, Task 2 auto)
- **Files:** 3 (1 created, 2 modified); +1519 / -33 lines
- **Commits:** 2 task commits (`704225c2f`, `1b72513cd`)

## Accomplishments

- `llm-endpoint-deploy env="dev" profile="ze-kasher-dev"`: rung 0 `mode`, then `env`, `pin`, `tarball`, `image`, `ladder`, `auth`, `ids` (local) and `account`, `s3-sha`, `digest` (read-only). `DRY_RUN=1` ends `DRY-RUN OK ... (offline rungs 1-7; account, s3-sha and digest NOT checked; ...)` with zero aws calls; `PREFLIGHT=1` ends `PREFLIGHT OK ... (rungs 1-10; read-only AWS calls only; ...)`; both flags is `REFUSED mode`. Live: create or update (`get-function` distinguishes absence by the exact `ResourceNotFoundException`), `wait function-active-v2` / `function-updated-v2`, optional `RESERVED_CONCURRENCY`, then a `readback` refusal unless Lambda holds 10240 / 900 / 10240 / arm64. The key hash reaches Lambda through a mode-0600 temp file and is printed only as `APR_API_KEY_HASH=set`.
- `llm-endpoint-prove-401` records `.ids/prove-401.json` and exits 3 on anything but 401. `llm-endpoint-url` refuses `proof` (no record, not 401, other digest, older than 24 h; all checked before or without creating anything) and `concurrency` (not 1-5 unless `ACCEPT_UNRESERVED_URL=1`, recorded as `risk_accepted`), creates the URL and BOTH statements, re-checks POST and GET `/health` without a key and removes everything with `URL REMOVED` on a non-401. Only the host is printed; the URL lives in the gitignored `.ids/url.json`.
- `llm-endpoint-teardown`: contain (concurrency 0, read back) -> URL config -> both `remove-permission` -> `get-policy` must list neither -> function -> the ONE recorded image tag, only while it resolves to the recorded digest (`SKIPPED image` otherwise) -> repository state and the `delete-repository` command printed under `NOT RUN` -> log group -> bucket only if the GGUF is its sole object -> CodeBuild project and role if `weights codebuild` made them -> function role. Spike-031 leftovers are listed and removed only with `TEARDOWN_SPIKE_031=1`.
- `up` (env-suffixed repository `aprender-llm-endpoint-<env>`, private encrypted tagged bucket, role from the trust/inline files, 7-day log group, `aws.json`), `weights` (copy or codebuild, `Metadata.sha256` checked against the ladder), `warm` (one authenticated 1-token invoke, key never in argv or output).

## Self-test table (`just llm-endpoint-selftest`)

```
PASS 1  pin file absent -> REFUSED pin
PASS 2  tarball sha mismatch -> REFUSED tarball
PASS 3  image-local.json with a wrong version line -> REFUSED image
PASS 4  APR_API_KEY_HASH unset -> REFUSED auth
PASS 5  64 chars that are not hex -> REFUSED auth
PASS 6  aws.json absent -> REFUSED ids
PASS 7  DRY_RUN=1 together with PREFLIGHT=1 -> REFUSED mode
PASS 8  DRY_RUN=1, consistent fixtures -> DRY-RUN OK (offline rungs 1-7; ... NOT checked ...), zero aws calls
PASS 9  PREFLIGHT, sts account differs from aws.json -> REFUSED account
PASS 10 PREFLIGHT, right account, wrong object sha -> REFUSED s3-sha
PASS 11 PREFLIGHT, right account and sha, tag moved -> REFUSED digest
PASS 12 PREFLIGHT, all three reads right -> PREFLIGHT OK (rungs 1-10; read-only AWS calls only; ...), three reads
PASS 13 NEGATIVE CONTROL: PREFLIGHT with the refusing shim -> REFUSED account and a recorded call
PASS 14 prove-401 on a 200 payload -> exit 3 and 401 NOT PROVEN
PASS 15 prove-401 on a 401 payload -> exit 0, 401 PROVEN and a record
PASS 16 url without prove-401.json -> REFUSED proof, zero aws calls
PASS 17 url with a valid proof, concurrency None -> REFUSED concurrency, no URL created
PASS 18 teardown: both statements removed, one tag deleted, no delete-repository (printed under NOT RUN)
PASS 19 teardown: tag moved -> SKIPPED image, no batch-delete-image
PASS 20-24 url: success path / open answer -> URL REMOVED / 48 h proof / other digest / ACCEPT_UNRESERVED_URL
PASS 25 up: env-specific repository, posture calls, aws.json that DRY_RUN deploy accepts
PASS 26-28 deploy live: create path, update path + RESERVED_CONCURRENCY, REFUSED readback
PASS 29-31 teardown: REFUSED policy, TEARDOWN_SPIKE_031 on/off
PASS 32-33 warm: key in no argv/output; REFUSED probe-key
PASS 34-37 weights copy (staged / REFUSED source-meta) and codebuild, teardown of the CodeBuild resources
PASS 38-41 url 200 proof; bucket with other objects kept; containment readback 5; foreign account
AWS CALLS: 0
FIXTURE READS: 9
LLM-ENDPOINT SELFTEST OK 41/41
```

Plan floor was 19 rows; 22 were added because the live paths (deploy create/update, url success and removal, up, weights, warm) otherwise ran for the first time against a real account.

## Mutation probes (CLAUDE.md Verification Discipline #4 / #7)

Eighteen guard mutations were applied one at a time to the justfile and the self-test required to turn red. Fourteen were caught immediately: url (digest compare, 24 h limit, concurrency gate, removal on non-401, second statement), teardown (second `remove-permission`, recorded-digest guard, `get-policy` check, executed `delete-repository`, spike guard), prove-401 always proven, deploy readback, up shared repository, weights source hash, warm key in argv. Four SURVIVED (url 401-status check, bucket sole-object guard, containment readback, teardown account guard); cases 38-41 were added and all four now turn red. Earlier, six task 1 mutations (aws call before DRY-RUN OK, hex alphabet, digest, account, s3-sha, mode guard) were caught.

## bashrs (`bashrs lint` on each extracted body; 0 errors everywhere)

| Recipe | errors | warnings | notes |
|---|---|---|---|
| llm-endpoint-image | 0 | 36 | was 16 errors (09-04's recipe, see Deviations) |
| llm-endpoint-up | 0 | 10 | |
| llm-endpoint-weights | 0 | 22 | |
| llm-endpoint-deploy | 0 | 35 | |
| llm-endpoint-prove-401 | 0 | 8 | |
| llm-endpoint-url | 0 | 27 | |
| llm-endpoint-warm | 0 | 7 | |
| llm-endpoint-teardown | 0 | 42 | |
| llm-endpoint-selftest | 0 | 67 | SEC020 on the shim shebang text |

Warnings are bashrs style/determinism items (DET/REL/SEC on `mktemp`, `date`, `read`, unquoted `$(...)` word splits in the spike loops). bashrs exits 1 on any warning, so the verify keys on the `N error` summary as the plan's grep does.

## Task Commits

1. **Task 1 (tracer): llm-endpoint-deploy ladder and the shimmed self-test** - `704225c2f` (feat)
2. **Task 2: up, weights, prove-401, url, warm, teardown, 28 more self-test cases, README, event template** - `1b72513cd` (feat)

Tracer gate: the Task 1 verify (self-test, missing-input refusal, mode refusal, `just --dump` structure, acceptance greps) was re-run end-to-end after six mutation probes and passed before expansion.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] bashrs false positives constrain how the recipes are written**
- **Found during:** Task 1/2 lint
- **Issue:** The task 2 verify requires 0 bashrs errors per body. bashrs 6.66.3 reports lexer false positives (SC1028/SC1035/SC1078/SC1007/SEC002/SEC011) for: parentheses inside a double-quoted string on a line holding `[`; an apostrophe in a double-quoted string; `${#VAR}`; heredocs; a keyword (`in`, `fi`, `esac`) directly before a closing quote; `in` inside a hyphenated word (`--retention-in-days`, `raw-in-base64-out`); a `"$(cmd \<newline> args)"` substitution; `docker ... "$(...)"`.
- **Fix:** Reworded messages, replaced `${#KEYHASH}` with `wc -c`, replaced heredocs with `python3 -I -c`, moved the `read`-style continuation into single lines with an `AP=(--profile ... --region ...)` array, set log retention through `--cli-input-json`, and built the CLI payload-format value in two pieces (`BINFMT="raw-i""n-base64-out"`). Behaviour is unchanged; the runtime was exercised by the self-test after each rewrite.
- **Files modified:** justfile
- **Commits:** 704225c2f, 1b72513cd

**2. [Rule 3 - Blocking] `llm-endpoint-image` (plan 09-04's recipe) edited to lint clean**
- **Found during:** Task 2 verify (its loop names `llm-endpoint-image`)
- **Issue:** 16 bashrs errors from the same false-positive classes.
- **Fix:** message rewording (parentheses and apostrophes), the two heredocs replaced by `python3 -I -c`, the `docker image inspect --format` result read through a file, one multi-line `$(aws ...)` made single-line. Re-ran the real recipe afterwards: `IMAGE OK aprender-llm-endpoint:cpu-v0.71.4 (apr apr 0.71.4 (5027da624))`, rc 0.
- **Files modified:** justfile
- **Commit:** 1b72513cd

**3. [Rule 2 - Missing critical] Account guard in every writing recipe; repository consistency in rung 10**
- `up`, `weights`, `url` and `teardown` compare the sts account with `aws.json` before writing or deleting (plan only had it as deploy rung 8); deploy rung 10 also refuses when `image-pushed.json` names another repository than `aws.json` (a teardown/deploy across environments cannot silently succeed). Cases 41 and the `digest` mutation cover them.

**4. [Rule 2 - Missing critical] Spike-031 ECR repository is never deleted by the recipe**
- The plan lets `TEARDOWN_SPIKE_031=1` execute the spike leftovers while its own prohibition and verify forbid any non-echo `delete-repository`. The two conflict only for the spike repository; the recipe removes every other leftover and always prints the repository command (case 30).

**5. [Rule 3 - Blocking] Third test seam `LLM_ENDPOINT_BIN_DIR`**
- The plan named two seams; the tarball path (`deploy/llm-endpoint/bin/$APR_ASSET`) would otherwise need the self-test to write into the real gitignored `bin/`. A third seam keeps the self-test out of the repository tree. Default unchanged.

**6. [Rule 1 - Bug in the plan text] `just --show` does not expand `{{ }}`**
- The task 2 verify says just's interpolations are expanded by `--show`. They are not: bodies keep `{{ trim_start_match(...) }}` and `{{profile}}`, which bashrs accepts. `--dry-run` shows the expanded form; the image recipe's `{{{{.Id}}` was checked there. No code change needed.

**7. Wording**
- The plan's tail header `NOT RUN — the human's call` became `NOT RUN -- for the human to decide` (an apostrophe and an em dash inside a double-quoted echo trip the lexer); the verify greps only `NOT RUN`.

**Total deviations:** 7 (4 blocking, 2 missing-critical, 1 plan-text note). **Impact:** none on scope; the plan's acceptance criteria all pass. The extra cases are additive.

## Authentication Gates

None. No AWS call was made (the shim is the only `aws` the recipes reach); `gh release download` and `docker buildx` ran unauthenticated-capable for the image re-run.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-09-18/19/20/21/48/49 mitigations are implemented as specified and each is pinned by cases: 401 gate and removal (16, 17, 20-24, 38), both statements and `get-policy` (18, 21, 29), key/URL not printed (20, 26, 32), shim and counters (13), env-specific repository and tag-only deletion (18, 19, 25), concurrency gate (17, 24).

## Issues Encountered

- The worktree harness rewrites some git porcelain through `rtk` and refuses compound shell commands; staging and committing used `/opt/homebrew/bin/git`, and multi-step verifies ran as scripts from the session scratchpad instead of `/tmp/p09-06-*`.
- `just --show` bodies need the `sed 's/^    //'` extraction the plan describes; bashrs line numbers refer to that extracted file.
- Case 37's first version grepped the argv with the wrong flag order; the recipe was right and the case was fixed (found through `SELFTEST_KEEP=1`, which keeps the temp directory and was added for that purpose).

## Next Phase Readiness

- Plan 09-07's go/no-go can run `PREFLIGHT=1 just llm-endpoint-deploy` after `just llm-endpoint-up` and `just llm-endpoint-image 1`; the first real AWS run is where the aws-cli argument shapes get checked against the service (D5 human_judgment).
- Plan 09-13 reads `.ids/function.json` (`function`, `image_digest`, `reserved_concurrency`) and `.ids/url.json`; `RESERVED_CONCURRENCY=1` at deploy is what lets `llm-endpoint-url` pass its concurrency gate.
- The README still mentions the probe/measure recipes by name only; those are 09-13's.

## Self-Check: PASSED

- FOUND: justfile, deploy/llm-endpoint/README.md, deploy/llm-endpoint/events/apigw-v2-no-auth.json
- FOUND (ancestors of HEAD): 704225c2f, 1b72513cd
- Commits measured from the ledger: 2 (`2bdca505b..1b72513cd`) before this SUMMARY's own commit
- Task 1 and Task 2 verify blocks and acceptance greps re-run after the final edit: pass (41/41, AWS CALLS: 0)
