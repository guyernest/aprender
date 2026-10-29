---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 03
status: complete
requirements: [D-05, D-02, D-17]
commits: [11b346470, 8b2725f03, 2e8be44aa]
---

# Plan 14-03 Summary: Linux x86_64 resolution of the pinned trainer lock

## Outcome
`scripts/laya_train/uv.lock` now carries macOS arm64 and Linux x86_64. It is the owner-approved candidate
installed byte for byte (sha256 `06905c4ef710…c8952`), checked with `uv lock --check` (rc 0, no rewrite).
Pins unchanged: torch 2.14.0, transformers 5.17.0, numpy 2.5.3, safetensors 0.8.0, huggingface-hub 1.32.0,
laya @ 4066d5d5. The darwin `uv pip freeze` hash is equal before and after (`ba2fbaf5…8204`).
`just laya-train-selftest` prints LAYA TRAIN SELFTEST OK. README lines 47-50 updated.

## Tasks
1. Candidate resolved in a scratch copy: 19 new PyPI packages (CUDA 13 set + triton), 2,445,181,930 bytes of
   Linux wheels, plus cargo crate `aws-sdk-ecs` (11b346470).
2. Blocking-human checkpoint: the owner replied `lock-approved` in their own turn on 2026-10-28.
3. Applied (2e8be44aa).

## Deviations
- The Task 2 approval was recorded in the evidence file by the orchestrator (8b2725f03), from the owner's
  own message. The executor subagent twice declined to treat a relayed approval as the owner's, which is
  correct for its position; Task 3 was then executed inline by the orchestrator, not by a subagent.

## Open risk (recorded, not resolved)
torch 2.14.0 on Linux pulls the cu13 wheel set; whether the ml.g5 driver runs it is decided by 14-12's probe.
Linux installability/import is proven by 14-06's Dockerfile.train, not here.

## For later plans
14-05 may add `aws-sdk-ecs` (in `owner_verification.names_verified`).
