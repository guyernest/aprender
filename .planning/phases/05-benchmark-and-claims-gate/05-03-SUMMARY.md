---
phase: 05-benchmark-and-claims-gate
plan: 03
subsystem: training
tags: [setfit, calibration-regime, epsilon-basis, contract-gate, halted, enospc, host-blocker]

requires:
  - phase: 05-benchmark-and-claims-gate
    provides: "05-01's 12 banked calibration passes (the measurement bank this plan derives candidates from)"
  - phase: 05-benchmark-and-claims-gate
    provides: "05-02's per-regime RegimeThresholds / table_for restructure and the deliberate sole() tripwire"
  - phase: 05-benchmark-and-claims-gate
    provides: "05-14's fail-closed epsilon_basis derivation this plan must turn GREEN"
provides: []
affects: [05-07, 05-09, 05-11, 05-12, 05-13]

tech-stack:
  added: []
  patterns: []

key-files:
  created: []
  modified: []

decisions:
  - "HALTED BEFORE Task 1 on a host blocker, not on the D-04 decision checkpoint. Nothing was committed except this SUMMARY; the decision memo was NOT written, because its central tables are required by the plan to be derived BY RUNNING the shipped combine and no build can run on this volume"
  - "Did NOT delete the user's 138 GB build cache. STATE.md carries an explicit standing instruction from the two prior occurrences of this exact blocker: the executor reports and halts, the coordinator reclaims. A worktree-isolated sub-agent reclaiming 117 GB from the MAIN checkout's target tree is also outside its blast radius"
  - "Did NOT hand-derive the candidate lower bounds. Plan step (1) says 'never by hand arithmetic' and threat T-05-03-05 names hand-derivation as the tampering vector this plan exists to prevent — a memo built by hand would be the precise failure the gate is designed to catch"
  - "Did NOT reuse a prebuilt pv or test binary. The plan's own verify block forbids it by name (CLAUDE.md rule 8, shadowed artifact): a stale target/release/pv validating the new per-regime block and passing is the highest-risk failure of the phase"

metrics:
  duration_seconds: 900
  tasks_completed: 0
  files_changed: 0
  completed: 2026-08-17

actuals:
  tokens: 0
  tasks: 0
  commits: 1

status: blocked
---

# Phase 5 Plan 03: Epsilon Basis Decision — HALTED (host blocker, nothing landed)

**The volume is full — 0 GiB available of 926 GiB — so not one of this plan's machine-checked
criteria can execute, and the plan's central artifact cannot be honestly produced. No contract,
threshold table, constant, test or decision memo was written or committed. The D-04 checkpoint was
never reached.**

## STATUS: BLOCKED before Task 1 — reported, not worked around

This is the third occurrence in this milestone of the recurring ENOSPC blocker that `CLAUDE.md` and
`STATE.md` both name. It is reported the same way the previous two were, per the standing
instruction recorded in `STATE.md`.

## The blocker, measured

Every status below was read directly, never through a pipe (CLAUDE.md verification rule 1).

| Probe | Command | Result |
|---|---|---|
| Free space (human) | `df -h /Users/guy` | **276 MiB**, then 596 MiB — 100% capacity |
| Free space (GiB) | `df -g /Users/guy` | **`Available` = 0**, 903 of 926 used |
| Main checkout build cache | `du -sh …/aprender/target` | **138 GB** |
| — debug subtree | `du -sh …/target/debug` | **117 GB** |
| — **incremental cache** | `du -sh …/target/debug/incremental` | **90 GB** |
| — release subtree | `du -sh …/target/release` | 1.6 GB |
| This worktree's build cache | `du -sh <worktree>/target` | **rc=1 — does not exist** |
| Target-dir redirect | `printenv CARGO_TARGET_DIR` | **rc=1, unset** |
| Target-dir redirect (file) | `cat …/aprender/.cargo/config.toml` | **rc=1, absent** |
| Harness disk preflight | `evidence.rs:3328` | **`const MIN_FREE_GIB: u64 = 10;`** — asserts `avail_gib >= MIN_FREE_GIB` and refuses to start a pass |

**The 90 GB `target/debug/incremental` figure is the actionable one.** `CLAUDE.md` and `STATE.md`
record this cache regrowing to "~25 GB and filling the volume", twice, in Phase 2. It is now at
**~3.6x that recorded worst**, which is why the volume is at zero rather than merely tight.

### Why the mechanism is proven rather than assumed (CLAUDE.md rule 2)

- `cargo metadata --no-deps` exits **rc=0**, so cargo itself is healthy. The constraint is disk
  alone, not a broken toolchain — worth stating, because "cargo is broken" and "the disk is full"
  have different remedies.
- This worktree has **no `target/` directory at all**, and there is no `CARGO_TARGET_DIR` and no
  `.cargo/config.toml` redirect. So `cargo test --release -p aprender-train --lib --features setfit`
  and `cargo run --release -p aprender-contracts-cli --bin pv` must both compile from scratch here.
  Against `Available = 0` that is not a marginal call.
- The harness's own `MIN_FREE_GIB = 10` preflight **fails closed** below 10 GiB. Even with a build
  already in place, a calibration pass would refuse to start at 0 GiB. The code's own gate agrees
  with the diagnosis.
- The session's tooling is already degraded by it: a `Read` of `05-14-SUMMARY.md` failed with
  `jq: error: writing output failed: No space left on device`.

**I deliberately did NOT run a doomed 78-crate build to "prove" the failure.** It would have
consumed the last few hundred MiB of a volume already at 100%, risking wedging the host for the
user, and the four independent measurements above already establish the fact. Spending the last of
a full disk to produce an error message that is already implied is not evidence-gathering.

## What was NOT done, and why each refusal was correct

| Not done | Why |
|---|---|
| The decision memo `05-03-epsilon-basis-decision.md` | Plan step (1): "Derive the candidate lower bounds by **RUNNING the shipped combine, never by hand arithmetic**." Threat **T-05-03-05** names hand-derivation as the tampering vector. A memo assembled by hand is the exact artifact this plan's controls exist to reject |
| A *partial* memo (L1 only, L2/L3 marked pending) | The `CANDIDATE LOWER BOUNDS` header would exist with no candidates under it, satisfying the artifact's `contains:` criterion while carrying none of its content. That is the hollow-green shape this phase has already caught three times (CR-02). The D-04 checkpoint would then be reviewing an incomplete basis while looking complete |
| Deleting `target/debug` to unblock | `STATE.md`, on this exact blocker: *"Nothing was deleted by the executor (standing instruction) and the run was reported as a checkpoint with measured numbers; the coordinator reclaimed ~21 GB."* Reclaiming 117 GB from the **main checkout** is also outside a worktree-isolated sub-agent's blast radius, and forces a full debug rebuild of a 78-crate / 25,300-test workspace the user did not ask for |
| Reusing an existing `pv` or test binary | The plan's verify block forbids it by name: *"pv is invoked through cargo, never as a path… a STALE `target/release/pv` validating the new per-regime block and passing is the shadowed-artifact failure (rule 8) on the highest-risk edit of the phase"* |
| The `sole()` migration / D-17 correction (rule-independent work) | Real work, but unverifiable here — every gate that would prove it is a build. The plan holds it uncommitted until after D-04 anyway (Task 3). Committing unverified code edits to bank partial credit is what the one-commit shape exists to prevent (Pitfall 1) |

## Verification

| Check | Command | Result |
|---|---|---|
| Worktree base correct | `git rev-parse HEAD` | `5a5231b0b…` — matches expected base |
| Branch in agent namespace | `git rev-parse --abbrev-ref HEAD` | `worktree-agent-a4dfa89524b391d4d` |
| Working tree clean, nothing half-written | `git status --porcelain` | empty (rtk renders this as `ok`) |
| 05-01's evidence bank intact | `ls …/calibration-store/` | **12 passes present** (24 files: 12 `*.evidence.json` + 12 `*.meta.json`) — nothing this plan needed was lost |

**Nothing was left in a partial state.** The 12 banked calibration passes are untouched and
committed, so the moment disk is reclaimed this plan resumes at Task 1 step (1) with zero
re-measurement — no compute has to be re-spent.

## What unblocks this

One of:

1. **Reclaim the incremental cache** (recommended, and the precedent from the two prior
   occurrences): `target/debug/incremental` is 90 GB and is regenerable build state, not evidence.
   `CLAUDE.md` already prescribes `export CARGO_INCREMENTAL=0` on this host for exactly this reason,
   and every command this plan runs is release-profile, so a debug cache buys this plan nothing.
2. Reclaim elsewhere on the volume to clear the ~10-20 GiB a from-scratch release build of
   `aprender-train --features setfit` plus `aprender-contracts-cli` needs, plus the harness's
   `MIN_FREE_GIB = 10` floor.

Then re-dispatch this plan unchanged. It has no partial state to reconcile.

## Downstream impact

**F-10 remains blocked, so no user-reachable path produces a `setfit-apr-v1`.** Per the plan's own
option-F wording, the plans that stay blocked are:

- **05-07** and **05-12** — directly blocked on the production regime entry
- **05-09**, **05-11**, **05-13** — blocked transitively

This is the same blockage option F describes, reached for a different reason: option F is a
*decision* to halt on the evidence, whereas this is an *inability to reach the decision at all*. The
D-04 checkpoint has not been presented and no basis has been selected, so nothing about the epsilon
question has been prejudged.

## Deviations from Plan

None. No plan step was altered, substituted or partially executed. Task 1 could not begin.

## Known Stubs

None — nothing was written.

## Threat Flags

None. No file outside this SUMMARY was created or modified; no contract, threshold, constant or test
was touched; no dependency was installed.

## Self-Check: PASSED

| Claim | Verified |
|---|---|
| `05-03-SUMMARY.md` exists | FOUND |
| No contract / threshold / evidence file modified | `git status --porcelain` empty before this SUMMARY |
| `calibration-store/` still holds 12 passes | 24 files listed |
| Free space really is exhausted | `df -g` → `Available` = 0, confirmed twice |
| `MIN_FREE_GIB = 10` preflight exists as cited | `evidence.rs:3328` |
