---
status: testing
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
source: [14-VERIFICATION.md]
started: 2026-10-31T05:26:59Z
updated: 2026-10-31T05:26:59Z
---

## Current Test

number: 1
name: Run the packing cancel on a run that passes its gate (truth 8)
expected: |
  ECS pack task stopped, lock released, task cancelled, no .apr
awaiting: user response

## Tests

### 1. Run the packing cancel on a run that passes its gate (truth 8)
expected: ECS pack task stopped, lock released, task cancelled, no .apr (about 62 min of paid compute; owner candidate, not a Phase 15 criterion)
result: [pending]

### 2. Accept the deferral of truths 12 and 13 to Phase 15
expected: Phase 15 reaches a D-16 reproduction with next_trusted true; until then 14-GATE-RUN-EVIDENCE.json reads not-reproduced and the server is not trusted. No waiver (14-GAP-ROUND-2-OWNER-DECISIONS.md)
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
