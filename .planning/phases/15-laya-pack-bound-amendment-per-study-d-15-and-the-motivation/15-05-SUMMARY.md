---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 05
subsystem: infra
tags: [laya, pack-image, codebuild, cdk, finalizer-provenance, stack-diff-prediction]
requires:
  - phase: 15-04
    provides: image_head 9b5e17101 (the committed, clean tree the pack image is built from) and 15-OFFLINE-GATE-EVIDENCE.json
provides:
  - 15-ROLL-RUNBOOK.md (owner commands for the image build, stack diff, deploy, outputs; re-pack placeholder)
  - 15-ROLL-EVIDENCE.json (pre-roll facts, new pack digest and commit, synth sidecar, leaf-level expected_change_set, owner reply)
  - a new pack image carrying amendment A2, built by the owner from the committed tree
  - the trainer stack synthesized with the new digest and a predicted diff for plan 15-06 to read the live diff against
affects: [15-06]
tech-stack:
  added: []
  patterns: [leaf-level offline template comparison with value hashes as a prediction to check a live diff against]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-ROLL-RUNBOOK.md
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-ROLL-EVIDENCE.json
  modified: []
key-decisions:
  - "Pre-roll and expected parity-contract hashes are recorded separately: pre_roll carries the A1 contract hash 7c646b74... (the image that refused attempt 1), expected carries the committed A2 hash 466891fa..., which plan 15-06 requires the re-pack to match"
  - "The state machine's LAYA_PACK_IMAGE_DIGEST override is part of the predicted change set (the plan said the state machine MAY change)"
requirements-completed: [D-03, D-04, D-15, D-17]
duration: n/a
completed: 2026-10-31
status: complete
actuals:
  tokens: 4000
  tasks: 3
  commits: 3
plan_head_before: 5d7538139c54c3bd24057f857ae1d6f5dd164b7b
plan_head_after: 0f0d68ad28b45964a9a85abfe75da83ae78d22ce
---

# Phase 15 Plan 05: Pack image rebuild and predicted stack diff Summary

**The owner rebuilt the pack image from the committed amended tree (sha256:dd38865c..., replacing 504084d0...); the trainer stack is synthesized with it, provenance-clean, a DRY_RUN deploy touches nothing, and the stack diff plan 15-06 will review is predicted leaf by leaf: three changed resources, none added or removed.**

## Accomplishments

- Task 1 (tracer): no pack-context path changed since image_head; `DRY_RUN=1 just laya-trainer-image pack dev ze-kasher-dev` ended `AWS CALLS: 0` naming HEAD; the pre-roll template (old digest) was synthesized and kept as the gitignored `models/laya-trainer/p15-template-pre.json`; the runbook (Sections A to D) and the evidence skeleton were written.
- Task 2 (checkpoint, owner): the owner ran Section A1 themselves. Last line `IMAGE OK pack sha256:dd38865c43aba2f3819e50addcf6db5e9da94ef84e762d93cf6b2c132f1d17e2`. The orchestrator recorded it verbatim in `owner_replies` (commit 79df1af16). The executor did not run, record or relay it.
- Task 3: images-dev.json `pack.digest` equals the reply and differs from the pre-roll digest. `pack.commit` is 5a1cb55b8, a planning-only descendant of image_head 9b5e17101, and `git diff --quiet` over the context paths holds. `just laya-trainer-synth dev <train> <new pack>` ended `TRAINER SYNTH OK env=dev` (sidecar commit 79df1af16, closure clean), `finalizer-provenance verify` ended `FINALIZER PROVENANCE OK`, and `DRY_RUN=1 just laya-trainer-infra deploy dev ze-kasher-dev` ended `AWS CALLS: 0` with no REFUSED line.
- Predicted change set (a prediction from two current-source synths; no resource added or removed; only the Resources section differs):

| Logical resource | Type | Changed leaf |
|---|---|---|
| PackTaskDefinitionCAD6B20B | AWS::ECS::TaskDefinition | `Properties.ContainerDefinitions[0].Image.Fn::Join[1][3]`: 504084d0... to dd38865c... |
| Machine6FC8AE80 | AWS::StepFunctions::StateMachine | `Properties.DefinitionString.Fn::Join[1][28]`: the `LAYA_PACK_IMAGE_DIGEST` container override, 504084d0... to dd38865c... |
| Finalizer7E8242E4 | AWS::Lambda::Function | `Properties.Code.S3Key` (new asset hash), plus the CDK asset metadata path naming the same asset |

## Task Commits

1. Task 1: 5a1cb55b8 (runbook and evidence skeleton)
2. Task 2: owner action; reply recorded by the orchestrator in 79df1af16
3. Task 3: 0f0d68ad2 (new image, synth sidecar, expected_change_set)

## Deviations from Plan

**1. [Rule 1 - verify strictness] Finalizer asset metadata leaf**
- **Found during:** Task 3
- **Issue:** the Finalizer resource has a second differing leaf outside Properties, `Metadata.aws:asset:path`, which the Task 3 verify (every Finalizer leaf under `Properties.Code`) would reject. It is the CDK asset metadata naming the same new asset as `Properties.Code.S3Key`; the Phase 14 redeploy runbook counts it as part of the one code change.
- **Fix:** recorded it under that entry's separate `metadata_leaves` key with a note, instead of dropping it, so nothing is hidden. `leaves` holds only the `Properties.Code.S3Key` leaf, and the plan's verify passed (`IMAGE ROLL STAGED sha256:dd38865c43ab changes=3 leaves=3`).
- **Files modified:** 15-ROLL-EVIDENCE.json
- **Commit:** 0f0d68ad2

**2. [Informational] The state machine is in the change set**
- The plan's truth said the diff "may also touch the state machine"; the offline comparison shows it does: only the `LAYA_PACK_IMAGE_DIGEST` environment override of the PackVerify container, the same digest as the image. It is recorded as a predicted leaf with a note. No IAM change appears.

**3. [Informational] A separate expected parity-contract hash**
- The plan's `pre_roll.parity_contract_sha256` (7c646b74...) is the A1 contract. The rebuilt image should carry the committed A2 contract, `466891faa5e3e1e092a5483bafcdf2ba8c5c835c9cdda5aaff896d2e893584e9`, so that value is recorded under `expected.parity_contract_sha256`, and plan 15-06's re-pack must match it.

## Auth gates

Task 2 is a `checkpoint:human-action` (blocking-human): the pack image build runs with the owner's credentials. It was handled as normal flow, not a deviation.

## Known Stubs

None. Section D of the runbook (the offline script regeneration and the re-pack run) is an explicit placeholder that plan 15-06 fills in.

## Threat Flags

None. No account id, ARN or repository URI is in either committed file (the verify scans both).

## Hand-off to plan 15-06

- The live `infra diff` must show exactly the three resources above with the leaves listed. STOP on any IAM change, any `[+]` or `[-]` resource, any leaf outside the list, or a live "from" digest for the pack image other than `sha256:504084d0c254af66e01b40f9f4679ba48fc09c4bf80386cc458a9388ae3090f3` (drift since the last deploy).
- Do not commit under `crates/`, `Cargo.toml`, `Cargo.lock` or `rust-toolchain.toml` before the owner's deploy, or `infra diff|deploy` will refuse on finalizer provenance (cure: a fresh synth with the new digests).
- The re-pack must finish before about 2026-11-28 (`sources.motivation.expires_about`).

## Self-Check: PASSED

- 15-ROLL-RUNBOOK.md and 15-ROLL-EVIDENCE.json exist; commits 5a1cb55b8, 79df1af16 and 0f0d68ad2 are ancestors of HEAD.
- The ROLL READY and IMAGE ROLL STAGED verifies printed, FINALIZER PROVENANCE OK and the DRY_RUN deploy `AWS CALLS: 0` were observed.
