---
phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin
plan: 03
subsystem: requirements-intake
tags: [intake, conformance, requester-contract, provenance, redaction]

# Dependency graph
requires: []
provides:
  - "11-BRIEF.md: the requester's brief body, verbatim, with a recorded sha256 header"
  - "11-BRIEF-INTAKE.json (embed-brief-intake-v1): thirteen quoted checks, quoted answers, function-name final value"
  - "11-INTAKE/MANIFEST.json (embed-intake-manifest-v1): checkout status hashes, copied tool files, redacted event paths"
  - "deploy/embed-endpoint/events/apigw-v2-platform-captured.json: sanitized platform event for POST /v1/embeddings"
affects: [11-07, 11-09, 11-10, 11-11, 11-12, 11-13]

# Actuals (#2632) — chars/4 over the committed content of this plan
actuals:
  tokens: 23070
  tasks: 2
  commits: 2

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Quote-or-unspecified: every check and non-unspecified answer carries a quote verified verbatim against the brief or a copied intake file"
    - "Copied requester files are byte-identical to the requester's HEAD (verified with git show and cmp)"

key-files:
  created:
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-BRIEF.md"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-BRIEF-INTAKE.json"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/MANIFEST.json"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/REQUEST.md"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/answers.md"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/aprender_conformance.rs"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/aprender-embed-conformance.just"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/embedding-providers.json"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/aprender-embed-reference-vectors.schema.md"
    - ".planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-INTAKE/aprender-embed-reference-vectors.sample.json"
    - "deploy/embed-endpoint/events/apigw-v2-platform-captured.json"
  modified: []

key-decisions:
  - "intake_state complete, material_complete true, acceptance_ready true: the brief, schema, event, tool identity and the caller and function-name quotes are all present and verified"
  - "function_name.final is aprender-embed-endpoint-<stage>: the requester states the allowlist prefix is aprender-embed-, which this default satisfies, so no owner-decision branch is taken"
  - "The tool identity is the requester's test file and recipe, pinned to requester HEAD 369e8e79b, where the copies were verified byte-identical"
  - "The 13 check ids are the requester's own check names (shape ... wrong-dimensions-400), and each quote is a verbatim line from the brief's Acceptance section"

patterns-established:
  - "Verify the verify: a plan verify that contradicts its own fails_when clause is corrected to the stated condition and the change is recorded"

requirements-completed: []  # Deliberately empty: this plan delivers the requester's inputs, not the requirements. R1, R5 and R7 stay open until plans 11-07, 11-11 and 11-13 consume the intake and the conformance run passes.

# Metrics
duration: unmeasured (resumed run)
completed: 2026-10-23
status: complete
---

# Phase 11: Requester intake (plan 03) Summary

**Requester intake complete and validated: thirteen quoted conformance checks, the aprender-embed- allowlist prefix, the IAM-role caller in the same account, and a sanitized platform event, with two envelope answers left unspecified by the requester**

## Performance

- **Duration:** unmeasured (resumed from a checkpoint; the checkpoint run did not record a start time)
- **Tasks:** 2 of 2 complete
- **Files:** 11 created, 0 modified in the repository

## Accomplishments

- Brief saved as `11-BRIEF.md` with a `sha256-of-body` header. The body (16,760 bytes) is byte-identical to the requester's `205-APRENDER-EMBED-HANDOFF.md` at `369e8e79b`.
- The thirteen named checks are recorded with their requester identifiers (`shape` through `wrong-dimensions-400`). Every check quote is a verbatim line from the brief's Acceptance section. `stated_count` (13) is quoted separately and equals `check_count`, so no owner decision on the count was needed.
- The conformance test file, the recipe block and `embedding-providers.json` are copied byte-identical from the requester's HEAD. The recipe copy was checked as the exact 20-line `aprender-embed-conformance` block.
- The platform event is sanitized and committed under `deploy/`. `requestContext.requestId` and `requestContext.http.sourceIp` are replaced with `REDACTED`. No account, identity, domain, API id or auth header was present.
- The function-name answer is recorded: prefix `aprender-embed-`, `final` = `aprender-embed-endpoint-<stage>`.
- Caller answer recorded: same account, principal kind IAM role.
- The intake manifest records the status hash and HEAD of both sibling checkouts as read in this run. Neither checkout was written to.

## Intake table

| Item | Status | Quotes | Note |
|------|--------|--------|------|
| 1. Brief (13 checks) | present | 14 (13 checks + stated_count) | body hash 99fcb856...; header line 1 |
| 2. Tool `aprender-embed-conformance` | present | 0 (tool identity recorded, not quoted) | pass line `CONFORMANCE RESULT PASS 13/13`; 3 copied files; pinned to 369e8e79b |
| 3. Reference-vectors schema | present | 1 | schema and sample copied; the reference JSON itself is NOT delivered |
| 4. Captured platform event | present (sanitized) | 1 (who-builds) | who builds: graph-rag's own tool; 2 redacted paths |
| 5. Function-name allowlist | present | 1 | prefix `aprender-embed-`; final `aprender-embed-endpoint-<stage>` |
| 6. Envelope-policy answers | present | 11 | 11 of 13 answered; 2 unspecified (see below) |

Checkouts read: `~/Development/mcp/sdk/pmcp-run` (HEAD 369e8e79b; the answers cite 21f1822a1 as HEAD at send time) and `~/Development/mcp/sdk/rust-mcp-sdk` (HEAD d034845, no match). `~/projects/mcp` is not a git repo and had no match.

## Unspecified answers (each is a downstream default)

- **usage_prompt_tokens (6a)**: the requester left it unspecified. The requester's suggestion is "count positions consumed after truncation, special tokens included", but that is a suggestion, not a stated policy, so it is not recorded as a value. Plans 11-07, 11-09 and 11-10 apply their own stated default and report it. This summary did not check which RESEARCH default applies.
- **unknown_fields (6e)**: the requester wrote "unspecified". Both ignoring and refusing are acceptable to them. The requester sends exactly `model` and `input`, which is recorded as a note.

## Function-name outcome

`function_name.final` = `aprender-embed-endpoint-<stage>`. The stages are `main` (dev: `us-east-1`, so the function is `aprender-embed-endpoint-main`) and `prod` (augury-dev: `eu-west-1`). No owner decision was needed, because the default already carries the required prefix. The three owner options apply only to a conflict, and there is none.

## Task Commits

1. **Task 1: requester intake material saved verbatim** - `36b1075ee` (docs)
2. **Task 2: normalized requester intake with quoted checks and unspecified answers marked** - `8510c97e7` (docs)

Plan metadata: this SUMMARY commit (docs, SUMMARY.md only).

## Files Created/Modified

- `11-BRIEF.md` - brief body verbatim, with `sha256-of-body` header
- `11-BRIEF-INTAKE.json` - normalized intake (embed-brief-intake-v1)
- `11-INTAKE/MANIFEST.json` - manifest (embed-intake-manifest-v1)
- `11-INTAKE/REQUEST.md` - the ask sent to the requester
- `11-INTAKE/answers.md`, `aprender_conformance.rs`, `aprender-embed-conformance.just`, `embedding-providers.json`, `aprender-embed-reference-vectors.schema.md`, `aprender-embed-reference-vectors.sample.json` - requester material, copied
- `deploy/embed-endpoint/events/apigw-v2-platform-captured.json` - sanitized captured event

## Decisions Made

- `intake_state: complete`, `material_complete: true`, `acceptance_ready: true`
- The tool identity is pinned to the requester's HEAD `369e8e79b`, where the copies were verified identical. The test file's last-change commit is `602efc9a6`.
- Check ids are the requester's own check names, not generated `check-NN` ids.

## Deviations from Plan

**1. [Rule 3 - Blocking] Brief moved to the phase root**
- **Found during:** Task 1 verify
- **Issue:** The delivered brief was placed in `11-INTAKE/`. The plan's artifact path and both verifies read it from the phase root.
- **Fix:** Moved to `11-BRIEF.md` at the phase root. Body bytes unchanged.
- **Commit:** `36b1075ee`

**2. [Rule 2 - Missing critical] Brief header with recorded body hash**
- **Found during:** Task 1
- **Issue:** The delivered brief had no `sha256-of-body` header, which the plan requires and the verify checks.
- **Fix:** Prepended a header line recording the source, the capture date and the sha256 of the body. The body was left untouched.
- **Commit:** `36b1075ee`

**3. [Verify correction] Function-name branch in the Task 2 verify**
- **Found during:** Task 2 verify
- **Issue:** As written, the verify demands `owner_decision` whenever `required_prefix` is set, even when the final name already carries the prefix. Its own `fails_when` names only a conflict between `final` and the prefix.
- **Fix:** Ran the verify with the branch applied only on a conflict (`final` not starting with `required_prefix`). Here there is no conflict. The verify was not otherwise weakened: the quote check on the function name still runs.
- **Suggested plan fix:** make the branch condition `not final.startswith(required_prefix)`.

**4. [Rule 2 - Missing critical] Event redaction**
- **Found during:** Task 1
- **Issue:** The plan requires the captured event to be sanitized. The delivered copy carries a `requestId` (`graphrag-embed-0`) and a `sourceIp`.
- **Fix:** Committed copy has `requestContext.requestId` and `requestContext.http.sourceIp` set to `REDACTED`. Both paths are listed in the manifest and the intake. The delivered copy `11-INTAKE/apigw-v2-platform-captured.json` is left untracked (it still has the unredacted `requestId`).

**Total deviations:** 4 (2 auto-fixed Rule 2 and 3, 1 verify correction, 1 redaction). **Impact:** no change to the requester's content. The brief body, the test file, the recipe block and the providers file are byte-identical to the requester's HEAD.

## Issues Encountered

- **Requester checkout drift.** The `pmcp-run` checkout is an active working tree (151 modified or untracked entries). Its status hash at the earliest read in the first run (`fc6aa0…`) differed from the later reads (`89ec5c…`), which match this run's reads. Its HEAD moved from `21f1822a1` (send time) to `369e8e79b`. Only read-only commands were used. The manifest records both hashes and this note. The `rust-mcp-sdk` checkout drifted the same way (`3c8975…` earliest, `87035d…` now). Not blocking, per the coordinator.
- **Brief version not checked at send time.** The brief is byte-identical to `369e8e79b`. The answers cite `21f1822a1` as the HEAD at send time, and that commit was not checked, so the brief's identity at that commit is unverified.
- **Live conformance not run (by instruction).** It needs AWS and a deployed function. The reference-vectors JSON has not been delivered (only the schema and a shape-only sample), so the run cannot pass yet.

## Owner command (not run)

From a checkout of `built-in/graph-rag` in the requester repository (the tool imports `snapshot_build`, so it does not run standalone):

```bash
cd built-in/graph-rag
just aprender-embed-conformance lambda://aprender-embed-endpoint-main/v1/embeddings /absolute/path/to/aprender-embed-reference-vectors.json
```

It must print `CONFORMANCE RESULT PASS 13/13`. It needs an AWS session for the dev account, a deployed `aprender-embed-endpoint-main` (plans 11-10 and 11-12), and the reference-vectors file (plan 11-07 or its successor).

## Self-Check: PASSED

- All 11 created files exist on disk (verified).
- Commits `36b1075ee` and `8510c97e7` are ancestors of HEAD (verified).
- Task 1 verify (plan's PYI1) passes: manifest, brief body hash, event redaction, checkout status equal before and after.
- Task 2 verify passes with the conflict-only function-name branch (deviation 3): `INTAKE OK state complete checks 13 material_complete True acceptance_ready True missing [] unspecified ['unknown_fields', 'usage_prompt_tokens']`.
- No secret-shaped string in committed content (scan of the copied files and event returned zero hits).

---
*Phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin*
*Completed: 2026-10-23*
