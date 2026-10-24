---
phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin
plan: 14
subsystem: testing
tags: [lambda, runtime-api, python, stdlib, harness, fixtures, embeddings]

requires:
  - phase: 11-01
    provides: scripts/embed_endpoint/ probes and the deploy/embed-endpoint/ directory (.ids convention)
provides:
  - Local Lambda Runtime API driver (MockRuntime, invoke_many, Session) with raw byte payloads
  - 14-case selftest with a stand-in runtime client, shown able to fail
  - Two credential-free API Gateway v2 embeddings events (one and three reference texts)
  - deploy/embed-endpoint/README.md with a status table later plans extend
affects: [11-06, 11-07, 11-11]

actuals:
  tokens: 6836
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Stdlib-only test drivers run under python3 -I with no sibling imports"
    - "Payloads: bytes sent verbatim, other values JSON-serialized"
    - "Each selftest case is isolated: a raising case is a FAIL, never a crash"

key-files:
  created:
    - scripts/embed_endpoint/mock_runtime.py
    - deploy/embed-endpoint/events/embeddings-single.json
    - deploy/embed-endpoint/events/embeddings-three.json
    - deploy/embed-endpoint/README.md
  modified: []

key-decisions:
  - "Session holds one child process across invokes; invoke_many queues a batch and kills the child after it. Plan 11-11 uses Session."
  - "Server side is HTTP/1.0 (one request per connection), which the pinned runtime client accepts."
  - "The broken-header control renames the server's request-id header only; the stand-in client spells its header names by parts, so the control cannot be neutralised by the same sed."
  - "Driver is standard library only and imports no sibling module; no account id, ARN or credential appears in any file."

patterns-established:
  - "Driver CLI: one JSON record per payload; exit 0 only when every record is a response"
  - "Event fixtures follow deploy/llm-endpoint/events/apigw-v2-no-auth.json"

requirements-completed: [R5, R1]

coverage:
  - id: D1
    description: "Local Runtime API driver: raw bytes verbatim, batch and persistent-session shapes, child lifecycle and environment isolation"
    requirement: "R5"
    verification:
      - kind: other
        ref: "python3 -I scripts/embed_endpoint/mock_runtime.py --selftest (14 cases)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Two platform-shaped embeddings event fixtures carrying the committed reference texts"
    requirement: "R1"
    verification:
      - kind: other
        ref: "event fixture assertions (version, rawPath, method, model, texts equal to full_model_reference.json)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Deploy directory README with driver instructions, .ids convention and status table"
    verification: []
    human_judgment: true
    rationale: "Prose runbook skeleton; its accuracy about later plans is a documentation judgment"

duration: unknown (start time not recorded for this run)
completed: 2026-10-23
status: complete
---

# Phase 11 Plan 14: Local Lambda Runtime API driver, embeddings events and deploy README

**A stdlib-only Lambda Runtime API driver that drives a real binary with hostile raw payloads, as a batch or as one persistent process, with a 14-case table that fails when the driver is broken**

## Performance

- **Duration:** unknown (start time not recorded)
- **Tasks:** 2
- **Files created:** 4

## Accomplishments

- `MockRuntime` serves GET next, POST response, POST error and POST init/error on an ephemeral 127.0.0.1 port, and spawns the binary with `AWS_LAMBDA_RUNTIME_API` and the `AWS_LAMBDA_*` function variables.
- Payloads are delivered byte for byte. The selftest verifies by sha256 and length: invalid UTF-8, an empty body, a 100,000-deep array, `{"a":1e999}`, and a 6 MiB body.
- `invoke_many` runs a finite batch. `Session` keeps one child alive across `invoke` calls, returns `no_answer` at once after the child dies, and kills the child on close.
- The selftest runs 14 cases against a Python stand-in client. All 14 pass. A copy with the server's request-id header renamed fails 11 of 14 cases and exits 1.
- Two API Gateway v2 events for `POST /v1/embeddings` carry one and three of the committed reference texts.
- The directory README records the driver usage, the gitignored `.ids/` convention and a status table.

## Task Commits

1. **Task 1: local Lambda Runtime API driver** - `cf5922760` (feat)
2. **Task 2: event fixtures and deploy README** - `0acdc86ee` (docs)

**Plan metadata:** committed separately as `docs(11-14)` (see Self-Check).

## Files Created/Modified

- `scripts/embed_endpoint/mock_runtime.py` - Runtime API driver, `invoke_many`, `Session`, CLI and `--selftest` case table with stand-in client
- `deploy/embed-endpoint/events/embeddings-single.json` - one-text platform-shaped event
- `deploy/embed-endpoint/events/embeddings-three.json` - three-text platform-shaped event
- `deploy/embed-endpoint/README.md` - directory purpose, driver usage, `.ids/` convention, status table

## Decisions Made

- `Session` and `invoke_many` are separate shapes because the plan 11-11 harness needs one live process across many checks, which a kill-after-batch API cannot give.
- Hostile payloads are sent as `bytes` and never JSON-wrapped, because the runtime deserializes the event before the handler runs. The driver therefore reaches failures a well-formed-JSON driver would never see.
- Negative control: the stand-in client builds its header names from parts, so renaming the server's header is a real fault and not something the same sed neutralises.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Selftest crashed instead of failing a case**
- **Found during:** Task 1, running the broken-header negative control
- **Issue:** The first version ran the cases as top-level statements. When the server's header was renamed, case 2 indexed a `None` body and the process crashed with a traceback. The control exited non-zero, but for the wrong reason, and a crash could hide a case that passed.
- **Fix:** Each case is now a function that returns `(ok, detail)`. A raising case records a FAIL, and the run continues to report every case.
- **Files modified:** `scripts/embed_endpoint/mock_runtime.py`
- **Verification:** The selftest reports 14 of 14 PASS. The broken-header copy reports `FAILED 3 of 14 cases passed` with individual FAIL lines and no traceback.
- **Committed in:** `cf5922760` (the fix was made before the Task 1 commit)

**Total deviations:** 1 auto-fixed (Rule 1). **Impact:** Fixed before commit; no scope change.

## Issues Encountered

- The plan's verify commands write logs under `/tmp`. The session's scratchpad directory was used instead. The checks themselves are unchanged.
- The Task 2 event fixtures use an `embed-local-*` request id and a `local` user agent. This is deliberate: no account or real request identifier appears in committed text.

## Scope Boundary: what this plan does not prove

- The selftest exercises the protocol with a Python stand-in client. It does not drive the Rust endpoint binary, which does not exist yet at this plan. The raw-payload behaviour of the pinned `lambda_runtime 0.13.0` was reproduced earlier in planning (per the plan's Facts), not re-run here.
- `REQUIREMENTS.md` was not updated by this executor. R5 and R1 are listed in `requirements-completed` as the plan requires, but the proof of R5 belongs to later plans.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plans 11-06, 11-07 and 11-11 can import `invoke_many` and `Session` from `scripts/embed_endpoint/` with an explicit `sys.path` insert.
- The two events are available as fixtures for the local invoke recipes.
- No blockers.

## Self-Check: PASSED

- Files on disk: `scripts/embed_endpoint/mock_runtime.py`, both event fixtures, `deploy/embed-endpoint/README.md`: FOUND
- Commits in history: `cf5922760` (driver), `0acdc86ee` (fixtures and README): FOUND
- Selftest: `MOCK RUNTIME SELFTEST OK 14 cases`, exit 0
- Broken-header control: 11 of 14 cases FAIL, exit 1
- Stdlib-only imports; no 12-digit sequence or `arn:aws` in any file this plan added
- STATE.md and ROADMAP.md not modified by this plan

---
*Phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin*
*Completed: 2026-10-23*
