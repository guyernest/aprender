# Phase 15: Laya pack-bound amendment per study (D-15) and the motivation-v1 re-reproduction (D-16) - Research

**Researched:** 2026-10-31
**Domain:** provable-contract amendment (`laya-parity-v1` pack/verify re-score bound), Rust verifier plumbing, justfile deploy gating, owner-run AWS reproduction (SageMaker A10G + Fargate Graviton pack)
**Confidence:** HIGH on the code/contract map and the evidence numbers (all read this session); MEDIUM on the owner-run AWS chain (derived from committed recipes, runbook and docstrings, not executed); LOW on whether D-16 will land in band (a single stochastic draw)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Already decided in Phase 14 (binding, not re-opened)
- **D-01:** Amend-per-study. The planner argues the per-set bound from `14-DRIFT-EVIDENCE.json`; the owner signs off at a `checkpoint:decision` before any contract edit. No contract edit precedes that sign-off. — **Reversibility:** one-way — the amendment changes a published provable contract (`laya-parity-v1`) that the pack task, `laya-verify` and downstream deploys re-read.
- **D-02:** D-16 is not waived and no `overrides:` entry stands in for it. The reproduction runs after the amendment lands, under a new declaration.
- **D-03:** Every AWS command is owner-run with the bang prefix; each paid run needs the owner's approval; owner replies are recorded on disk by the orchestrator, never relayed by an executor.

#### The bound: what it protects
- **D-04:** The Graviton (arm64 Fargate) pack task is the authority the amended bound is argued against. It is deterministic across repeats (one distinct value on all five legs). The Mac result is informational, not an input to the bound.
- **D-05:** Keep the amendment-A1 formula, `max(floor, k x training-device noise)` with a ceiling, but allow its parameters (k, floor, ceiling) to differ per set (`fine_tuned`, `zero_shot`). This addresses the case where 4 x the motivation-v1-sm zero_shot A10G noise (1.7265105816122173e-06) sits below the 1e-5 floor. — **Reversibility:** one-way — same contract edit as D-01.
- **D-06:** The bound must still refuse a real porting regression (served probabilities drifting enough to change decisions, e.g. a layout or kernel bug, anything approaching the 1e-3 ceiling). Platform float noise below that is acceptable. The plan states the red line it argued against.
- **D-07:** The checkpoint argues from existing evidence only. No new paid compute and no new Graviton or A10G measurement before the sign-off. The four `unmeasured` strings in `14-DRIFT-EVIDENCE.json` (x86_64, Graviton generation, fresh A10G run, second drifted set) are stated as limits of the claim. Fixing `pack_laya` to report every drifted set is NOT in this phase.

#### The role of the local `laya-verify`
- **D-08:** `just laya-verify` becomes advisory. The deploy and upload gate is the pack task's recorded verify (`pack-outcome.json`, accepted, `deploy_eligible` true). A Mac refusal is logged as a cross-CPU finding, consistent with the `must_note` already committed in `14-REPRODUCTION-DECLARATION.json`.
- **D-09:** The deploy and upload steps must also require that the local `.apr` sha256 equals the sha256 the pack task recorded, so the verdict is bound to the exact bytes.
- **D-10:** Phase 15 states the role in the contract text AND edits the `laya-deploy` and `laya-upload` justfile recipes so the tooling matches; it is not a doc-only change.

#### The new D-16 declaration
- **D-11:** The owner-approved band stays unchanged: margin [0.30, 0.45], post-calibration ECE [0.07, 0.12]. It was set before the first run and is not moved after seeing attempt 1 (margin 0.2860488947445468, ECE 0.09636022716760634).
- **D-12:** Attempt cap 1, no retry. A miss is recorded as not-reproduced (`next_trusted` false), the server stays untrusted, and the owner decides what follows. A failure before any gate verdict (crash, quota, timeout) does not consume the attempt.
- **D-13:** On a miss, nothing is pre-authorized (no follow-up data-growth phase is committed in advance). The amendment criteria (Phase 15 success criteria 1 to 3) can be met independently of D-16.
- **D-14:** The new run may differ from attempt 1. The declaration may permit any of: hyperparameters (epochs, learning rate, batch size, warmup), the seed set, eval-set size, and training data. Constraints: every change is listed with concrete values in the declaration and committed before the paid run; the planner proposes the values and the owner approves them; the evidence file states that the run is no longer an identical reproduction of attempt 1 or of Phase 12, so its claim scope is worded accordingly. Training-data changes stay bounded to what the declaration lists; building a general dataset-growth or annotation mechanism is a separate phase (see Deferred).

#### Compute sequencing
- **D-15:** Re-pack the already trained motivation-v1-sm under the amended bound (one Fargate pack, no GPU) after the amendment lands, to show an accepted deploy-eligible `.apr` from a CUDA checkpoint for criterion 3. It cannot close D-16 (its margin is out of band). Its S3 objects expire about 2026-11-28 (`sources.motivation.expires_about`), so this must run before then.
- **D-16:** Truth 8, the packing-stage cancel (about 62 min of paid compute), stays out of Phase 15. It remains open in `14-VERIFICATION.md` `human_verification`.
- **D-17:** Order: (1) bound `checkpoint:decision`, (2) contract and recipe edits, (3) re-pack motivation-v1-sm, (4) new D-16 declaration committed with the owner-approved training changes, (5) the single owner-run SageMaker attempt (about 1 hr of ml.g5.xlarge), (6) gate-run evidence with `next_trusted`.

### Claude's Discretion
- Concrete values for k, floor and ceiling per set, argued at the checkpoint (the owner signs, the planner proposes).
- The exact `pack-outcome.json` fields the deploy and upload recipes read, and how the sha256 comparison is implemented.
- Concrete proposed training changes for the declaration (D-14).

### Deferred Ideas (OUT OF SCOPE)
- A general dataset-growth and annotation-quality mechanism (the owner's attempt-1 note: "a mechanism to grow the dataset and improve annotation quality"): its own phase; D-14 permits only the bounded training-data change the declaration lists.
- Truth 8 packing-stage cancel live check (~62 min paid): remains an owner candidate in `14-VERIFICATION.md`.
- `pack_laya` reporting every drifted set (unmeasured[3]), x86_64 pack image, Graviton generation visibility: not planned.
- Review findings WR-01, WR-02, WR-03 and IN-01 to IN-09: open, awaiting owner triage.
- Explaining the MPS-versus-CUDA accuracy gap: not scheduled.
</user_constraints>

<phase_requirements>
## Phase Requirements

No requirement IDs are mapped to this phase (ROADMAP: "Requirements: TBD"). The planner maps plans to the five ROADMAP success criteria instead (SC1 to SC5, restated in the Validation Architecture section).
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- Contract work goes through `pv` (never a bash substitute): `pv validate`, `pv lint`, `pv diff` (two filesystem paths, materialise the old revision with `git show`). `pv` is NOT on PATH on this box; use `./target/release/pv` (v0.69.3, present) or `cargo run --release -q -p aprender-contracts-cli --bin pv --` as `just laya-claims-check` does. [VERIFIED: ran `./target/release/pv --version` this session]
- Code search: `pmat query`, not grep (grep is acceptable for YAML/justfile keys). `pmat query "rescore bound noise record" --path crates/aprender-decide` confirmed `rescore_bounds` is called from `cheap_checks` only (plus tests). [VERIFIED: pmat output this session]
- Every AWS command is owner-run (`!` prefix, profile `ze-kasher-dev`); compute above 1 hr on a non-lambda-vector host is a check-in (approved in principle 2026-10-30). Modifying `.github/workflows/*.yml` is a check-in (nothing here needs it).
- `unwrap()` is banned (`.clippy.toml`); clippy `-D warnings` per crate; `cargo fmt --all -- --check`. Mutation survivors on a PR diff are gated (`MUTANTS_MAX_MISSED`); every new refusal rule ships a test shown to discriminate by mutation (project convention, CLAUDE.md Verification Discipline 4 and 7).
- Verification Discipline: never read `$?` through a pipe; prove the mechanism engaged (trace line / digest / behaviour delta), not intent; pin binaries; a guard that does not scan the decision surface is theater.
- The Bash tool runs zsh (no word splitting, no `PIPESTATUS`); recipes themselves use `#!/usr/bin/env bash`.
- Justfile, not Makefile, for new scripts (user global); `bashrs` for shell lint (`bashrs lint`, `scripts/check_bashrs_gate.sh`).
- `crates/aprender-core/tests/readme_contract.rs` gates repo-relative paths cited in CLAUDE.md; if CLAUDE.md's Decision-model paragraph is updated, every path it cites must exist.
- CLAUDE.md line 249 states the bar as "max(1e-5, 4 x the per-checkpoint float64-referenced torch noise) (amendment A1, plan 08-13)"; it needs an A2 sentence after the amendment. The `scripts/laya_claims.tsv` anchors on CLAUDE.md are other sentences (safetensors scope, tier, bounds, hashes, ECE 0.0936), so editing the bar sentence does not break a claim. [VERIFIED: awk over scripts/laya_claims.tsv this session]

## Summary

The bound lives in exactly one contract (`contracts/laya-parity-v1.yaml`: `constants.pack_rescore_noise_k: 4`, `constants.pack_rescore_bound_max_abs: 1.0e-3`, `equations.pack_rescore_probs_abs.float_tolerance: 1.0e-5`) and is read at run time by exactly one production path: `aprender_decide::verify::VerifyPolicy::from_contract_views` -> `rescore_bounds` (called from `cheap_checks`) -> `rescore` per set, with the fine-tuned set re-scored FIRST and the zero-shot set second. Python (`scripts/laya_train/contract.py::noise_policy`, `train.py::write_noise_record`) reads the same three numbers to STAMP `rescore-noise.json`, and Rust requires the record's header `k` and `floor_abs` to equal the contract bit for bit and each set's reported `bound` to equal the Rust recomputation bit for bit. That equality is the central design constraint of this phase: a naive "per-set k and floor" edit makes every existing record (including the retained motivation-v1-sm run that criterion 3 must re-pack before ~2026-11-28) fail verification with `RescoreNoiseMismatch`.

Two further ripples decide the plan shape. (1) Both container images bake the contract at build time (pack image: `/src/contracts` plus a `pack_laya` compiled from the tree; train image: `COPY . /opt/laya/` with the contract in its context list), so the amended bound only reaches Graviton after an owner-run `laya-trainer-image pack` CodeBuild and a `laya-trainer-infra deploy` that rolls the new digest into the pack task definition. (2) `aprender-decide` is in the finalizer Lambda's `cargo tree` source closure, so any Rust edit there invalidates the finalizer provenance sidecar and forces a fresh `just laya-trainer-synth dev` (and a finalizer code-key change in the CDK diff) before `infra deploy` will run. Both are in the owner-run chain and must be staged.

The evidence supports an absolute-slack argument more than a larger-multiplier argument: on the three Graviton points the re-score drift is 1.5e-5 to 2.75e-5 in absolute terms but 3.2x to 10.0x in A10G-noise multiples, because the A10G noise term measures only torch's own CUDA-vs-float64 error and not the CPU-vs-CUDA gap. A1's k=4 was fitted when torch and Rust ran on the SAME aarch64 CPU (shared rounding walks, k_needed peaked at 1.18); that premise does not hold for CUDA-trained, Graviton-packed runs. The research below tabulates candidate (k, floor, ceiling) per set against every data point and states the red line; it does not pick values (the owner signs).

**Primary recommendation:** Amend additively (Option S-B below): add per-set flat constants to `laya-parity-v1` (version 2.0.0 -> 2.1.0 via `pv diff`), keep `pack_rescore_noise_k: 4` and `float_tolerance: 1.0e-5` as the record-stamp values and the absent-record floor, enforce the per-set bound in `rescore_bounds` only when a record exists, leave the trainer, the train image and `laya-finetune-gate-v1.yaml` untouched, rebuild only the pack image (plus the finalizer re-synth), and demote the Mac `laya-verify` to advisory ONLY for a `RescoreDrift` refusal when `pack-outcome.json` is present, accepted, deploy-eligible and its `apr_sha256` equals the local file's sha256.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Float64 noise record (`rescore-noise.json`) | Training device (SageMaker A10G, x86_64, CUDA) | — | `train.py::write_noise_record` is the only writer; it records torch's own error on the training device [VERIFIED: scripts/laya_train/train.py:574-609] |
| Bound derivation and enforcement (the authority) | Pack tier (Fargate Graviton arm64, `pack_laya pack` then `verify`) | Contract (`laya-parity-v1`) as the policy source | D-04: Graviton is the authority; the policy is read at run time from `/src/contracts` baked into the pack image [VERIFIED: deploy-extensions/container/pack-arm64/Dockerfile comments] |
| Verdict record | Pack tier (`run_step.py` writes `pack-outcome.json`) | Finalizer Lambda (copies hashes into manifest) | `accepted`, `deploy_eligible`, `apr_sha256` are written by `run_step.py` [VERIFIED: run_step.py:1297-1304, 1438-1446] |
| Deploy/upload eligibility | Local justfile recipes (`laya-deploy` step 5, `laya-upload`) on the Mac | Pack record (becomes the gate, D-08) | Recipes currently re-run the Rust verifier on the Mac and refuse on any refusal [VERIFIED: justfile:2516-2533] |
| Local `laya-verify` | Mac (arm64 M4 Pro) | — | Becomes advisory/cross-CPU finding (D-08), not an authority |
| Declaration and verdict for D-16 | `scripts/laya_deploy/laya_repro_eval.py` (pure evaluator) | Committed declaration JSON + gate-run evidence JSON in the phase dir | Evaluator is pure, reads only declaration + fetched run files [VERIFIED: laya_repro_eval.py docstring] |
| Sign-off ordering (SC1) | Planning artifacts (owner reply recorded on disk by the orchestrator) | git history | Contract edit must postdate the recorded reply |

## Standard Stack

No new libraries or packages. The phase edits existing code and contracts with the toolchain already pinned.

### Core
| Tool | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| Rust toolchain | 1.93.0 (`rust-toolchain.toml`) | `aprender-decide` verifier + `pack_laya` example | pinned; pack image builds with `rustc 1.93.0` and `--locked` [VERIFIED: `rustc --version`, Dockerfile] |
| `pv` | 0.69.3 (`./target/release/pv`) | `validate`, `lint`, `diff` of the contract | CLAUDE.md: the in-tree tool is the source of truth [VERIFIED: ran it] |
| `just` | 1.46.0 | recipes (`laya-verify`, `laya-deploy`, `laya-upload`, selftests) | project convention [VERIFIED: `just --version`] |
| `uv` + Python | 0.9.5 / 3.14.6 host (project venv under `scripts/laya_train`) | `just laya-train-selftest`, `laya_repro_eval.py` is stdlib-only | existing [VERIFIED: tool versions] |
| `bashrs` | 6.66.3 | lint of any generated owner script | CLAUDE.md [VERIFIED: tool version] |

### Supporting
| Tool | Version | Purpose | When to Use |
|------|---------|---------|-------------|
| `scripts/laya_deploy/laya_drift.py` | in-tree | writes the owner-run Graviton script (`aws-plan`), parses pack lines | re-pack of motivation-v1-sm (D-15); its `collect` merges into `14-DRIFT-EVIDENCE.json`, so write Phase 15 evidence to a NEW file |
| `scripts/laya_deploy/laya_repro_eval.py` | in-tree | `check-declaration`, `evaluate` | new D-16 declaration and verdict |
| `scripts/laya_deploy/laya_trainer_ops.py` via `just laya-trainer-image|infra|stage|call` | in-tree | image build, CDK deploy, dataset stage, train call | owner-run chain |
| `scripts/laya_deploy/laya_fetch.py` via `just laya-fetch` | in-tree | pulls a finished run to `models/decide/<name>/` | after the D-16 run |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Option S-B (additive per-set constants, record v1 unchanged) | Option S-A (record v2 with per-set `k`/`floor_abs`) | S-A changes `RescoreNoise` (`deny_unknown_fields`) in Rust, `write_noise_record` and `check_noise_record` in Python, `rescore_noise_schema` in `laya-finetune-gate-v1.yaml`, forces a train-image rebuild, and makes the retained motivation-v1-sm record (v1) unverifiable unless a v1 reader is kept, i.e. it puts criterion 3 at risk for no gain |
| Absolute per-set floor | Larger per-set `k` | k scales slack with noise: k=16 times the worst measured noise 6.27e-5 is 1.003e-3, over the 1.0e-3 ceiling; k=12 gives 7.5e-4 for noisy checkpoints (3x today's worst derived bound 2.5e-4) |

**Installation:** none. **Version verification:** not applicable (no external package is added; tool versions above were read from the binaries this session).

## Package Legitimacy Audit

No external packages are installed by this phase. Nothing was discovered via WebSearch or training data. `gsd-tools package-legitimacy check` was not run because there is nothing to check.

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
 owner reply (recorded on disk by orchestrator)  <-- checkpoint:decision: per-set (k, floor, ceiling) + S-B structure + laya-verify role
        |  (gates every edit below; git order is checked)
        v
 contracts/laya-parity-v1.yaml  --(pv validate/lint/diff, version bump)-->  committed
        |                       \
        |                        +--> contracts/aprender/binding.yaml notes, decide-apr-v1 verify_checks text, CLAUDE.md sentence
        v
 crates/aprender-decide  (VerifyPolicy per-set fields, rescore_bounds)  --cargo test--> green
        |
        |  git commit  (image builds read `git archive HEAD`; dirty closure is refused)
        v
 [OWNER] just laya-trainer-image pack dev ze-kasher-dev  ---> new pack image digest in images-dev.json
        |
 [CLAUDE, offline] just laya-trainer-synth dev   (decide is in the finalizer's cargo-tree closure => provenance forces a re-synth)
        |
 [OWNER] laya-trainer-infra diff -> (Claude reads) -> deploy   ---> pack task definition now points at the new digest
        |
        +--------------------------------------+
        v                                      v
 D-15 re-pack of motivation-v1-sm          D-16: new declaration committed (band unchanged, cap 1)
 (Fargate Graviton, runs/drift-.../,        |
  no GPU, before 2026-11-28)                [OWNER] stage dataset (if data changed) -> train call (SageMaker ml.g5.xlarge ~20-50 min)
        |                                    |
        v                                    v
 pack-outcome.json: accepted,           run: gate (Rust-recomputed) -> pack (Graviton, amended bound) -> verify
 deploy_eligible, apr_sha256                 |
        |                                    v
        |                              just laya-fetch -> laya_repro_eval.py evaluate -> 15-GATE-RUN-EVIDENCE.json (next_trusted)
        v
 laya-deploy / laya-upload (edited): pack-outcome accepted + deploy_eligible + apr_sha256 == local sha256  => proceed
        Mac `laya-verify` RescoreDrift refusal => logged as cross-CPU finding (not a gate); any other refusal still blocks
```

### Recommended change set (files the plans will touch)

```
contracts/laya-parity-v1.yaml                 # per-set constants, A2 amendment text, laya-verify role, version bump (via pv diff)
contracts/decide-apr-v1.yaml                  # verify_checks text at line ~388 (cosmetic but must not contradict)
contracts/aprender/binding.yaml               # pack_rescore_probs_abs / rescore_noise_reference notes (~2330-2345)
crates/aprender-decide/src/verify.rs          # ParityConstantsView, VerifyPolicy, from_contract_views, rescore_bounds
crates/aprender-decide/src/verify/tests.rs    # helper write_noise_record (reads policy.rescore_noise_k / rescore_probs_abs) + noise_* tests
crates/aprender-decide/examples/pack_laya.rs  # doc comment only (policy() is the one mapping)
justfile                                      # laya-deploy step (5), laya-upload eligibility block, laya-verify doc comment, selftest cases
CLAUDE.md                                     # Decision-model row sentence about the bar
.planning/phases/15-.../                      # decision record, declaration, evidence JSONs
NOT touched (by design): scripts/laya_train/*, contracts/laya-finetune-gate-v1.yaml, crates/aprender-mcp-laya-train*, the finalizer source
```

### Pattern 1: Where the bound is computed and enforced today (consumer map)

| Consumer | What it reads/does | Breaks if the keys change shape? |
|----------|-------------------|----------------------------------|
| `crates/aprender-decide/src/verify.rs` `ParityConstantsView` (`pack_rescore_noise_k`, `pack_rescore_bound_max_abs`), `ToleranceView` (`float_tolerance`), `from_contract_views` | typed view of the contract; no `deny_unknown_fields`, so ADDED keys are safe | removing/renaming a key breaks `pack_laya`, every test using `tests/common::policy()` |
| `verify.rs::check_noise_header` | requires `rec.k.to_bits() == policy.rescore_noise_k.to_bits()` and `rec.floor_abs.to_bits() == policy.rescore_probs_abs.to_bits()` | per-set k/floor in the policy fields make every v1 record refuse with `RescoreNoiseInvalid` field `k` or `floor_abs` |
| `verify.rs::rescore_bounds` | `floor.max(policy.rescore_noise_k * noise)`; reported `bound` must equal it bit for bit (`RescoreNoiseMismatch`); `within(bound, ceiling)` else `RescoreBoundCeiling`; absent record => `bound: floor` for BOTH sets | the central edit |
| `verify.rs::verify_loaded` | re-scores fine_tuned FIRST, drops the decider, loads the base, re-scores zero_shot second | none (but explains "first drifted set only") |
| `pack.rs::RescoreNoise` / `NoiseSet` | `#[serde(deny_unknown_fields)]`; `k`, `floor_abs` header, per-set `bound` | any record-schema change is a Rust + Python + gate-contract change |
| `scripts/laya_train/contract.py::noise_policy()` | returns `(k, floor, ceiling)` from the three keys through the typed `number` reader | trainer stamps header from these |
| `scripts/laya_train/train.py::write_noise_record` | writes `"k": k_mult, "floor_abs": floor`, per-set `"bound": max(floor, k_mult * max_abs)`; WARNs when `bound > ceiling` | image-baked; unchanged under S-B |
| `scripts/laya_train/lifecycle.py` (line ~608) | reads `float_tolerance` for the selftest re-score | selftest only |
| `scripts/laya_train/data.py` (~452-462) | selftest mutates `pack_rescore_noise_k` to test the typed reader | selftest only |
| `contracts/laya-finetune-gate-v1.yaml` `rescore_noise_schema` (lines ~545-556), `gate_pass.domain` (~779) | prose: "k ... must equal it bit for bit", "per-set bound(c, s) of laya-parity-v1 2.0.0 A1" | editing this file changes `gate_contract_sha256` in the train image vs the pack image; avoid it |
| `contracts/decide-apr-v1.yaml` ~388, `contracts/aprender/binding.yaml` ~2330-2345 | prose describing the bound | text only; update so they do not contradict |
| `deploy-extensions/container/pack-arm64/Dockerfile` | copies the three contracts into `/src/contracts` | needs a rebuild to carry the amendment |
| `scripts/laya_train/container/Dockerfile.train` + `context-paths.txt` | `COPY . /opt/laya/`; context includes the parity contract | stale contract inside the train image is harmless under S-B |

[VERIFIED: grep across crates/, scripts/, contracts/, justfile; verify.rs, pack.rs, contract.py, train.py read this session]. Production callers of `rescore_bounds`: `cheap_checks` only [VERIFIED: pmat query].

### Pattern 2: Option S-B, the additive per-set amendment (recommended structure)

**What:** Keep the three A1 scalars exactly as they are, with a narrowed meaning: `pack_rescore_noise_k: 4` and `float_tolerance: 1.0e-5` are the values the trainer STAMPS into the record header and the per-set reported `bound` (so records written by any existing trainer image stay valid), and `float_tolerance` remains the absent-record bound. Add per-set ENFORCED parameters as new flat constants (the file's own header says "FLAT scalars only", contract line 80), e.g. `pack_rescore_noise_k_fine_tuned`, `pack_rescore_floor_fine_tuned`, `pack_rescore_bound_max_abs_fine_tuned`, and the three `_zero_shot` twins. In `rescore_bounds`: header and reported-`bound` cross-checks stay against the stamp values; the enforced bound per set is `max(floor_s, k_s * noise)`, refused above `ceiling_s`.

**Why it is the safe structure:**
1. Old records stay valid, so the retained motivation-v1-sm run (S3 objects expire about 2026-11-28) can be re-packed for SC3.
2. The trainer, `train.py`, `lifecycle.py`, the train image and `laya-finetune-gate-v1.yaml` do not change (no train-image rebuild, no gate-contract hash divergence between train and pack images; IN-05 notes those hashes are recorded and never compared, but there is no reason to widen the gap).
3. The A1 invariant "omitting the record can only tighten the check, never loosen it" survives if each `floor_s >= 1.0e-5` and the absent-record bound stays `float_tolerance`: with a record the bound is `>=` the no-record bound. It also keeps `fail_closed_vectors` valid (its two vectors carry no record and must still refuse at the 1e-5 floor; that test is env-gated and SKIPs in CI, so a regression would be silent). [VERIFIED: crates/aprender-decide/tests/fail_closed_vectors.rs:1-40]
4. `ParityConstantsView` has no `deny_unknown_fields`, so an OLD `pack_laya` binary reading the NEW contract still parses (rollback-safe). [VERIFIED: verify.rs:470-486]

**Cost:** two meanings for "k" and "floor" in one file; the contract prose must say which is the stamp and which is enforced. The record's reported `bound` is no longer the enforced bound, only the stamp-formula bound; the contract invariant text must say so.

**Alternative S-A (not recommended):** put per-set `k`/`floor_abs` in the record. Rejected for the reasons in the Alternatives table; list it at the checkpoint so the owner sees it was considered.

### Pattern 3: The advisory `laya-verify` (D-08 to D-10) in the recipes

**Today:** `laya-deploy` step (5) and `laya-upload` run `just laya-verify "$APR" run data base`, refuse on a non-zero exit, then require the last JSON line to have `deploy_eligible` true and `artifact_sha256` equal to the file's sha256. [VERIFIED: justfile:2516-2533; upload block read via sed]. The sha comparison today is local-vs-local (verify hashes the same file), so it adds no binding to what the pack task judged.

**Fetched layout:** `laya_fetch.py::dest_for` maps a top-level manifest entry to `logs/<entry>`, so a fetched run's record is at `models/decide/<name>/logs/pack-outcome.json`, next to `<name>.apr`, `run/`, `data/` [VERIFIED: laya_fetch.py:146-158]. The recipes take `run` (`.../run`) as an argument, so the record path is `dirname(run)/logs/pack-outcome.json` by convention; add an explicit override (env var or trailing argument) rather than relying only on the convention.

**Fields to read (all present in the fixture and the writer):** `schema == "laya-train-pack-outcome-v1"`, `accepted == true`, `deploy_eligible == true`, `apr_sha256` (a 64-hex string, null unless accepted), optionally `pack_image_digest`. [VERIFIED: run_step.py:1297-1304; crates/aprender-mcp-laya-train/fixtures/pack-outcome-v1.json]. `apr_sha256` is set only when `accepted` (`apr_sha256=local_sha if accepted else None`, run_step.py:1440).

**Recommended gate logic (decision for the planner to put to the owner):**
1. Record present and valid (accepted true, deploy_eligible true, `apr_sha256` equals `shasum -a 256 "$APR"`) -> the record is the gate. Still run `laya-verify`; if it refuses with `REFUSED RescoreDrift ...` only, write a cross-CPU finding line to the state dir and continue; if it refuses for ANY other rule (load ladder, manifest binding, `GateFailed`, `SyntheticNotDeployable`, base/hash mismatch), still refuse, because those are not device-float-noise outcomes and demoting them would turn "advisory" into "unchecked".
2. Record present but not accepted, not deploy-eligible, or sha mismatch -> `REFUSED eligibility ...`.
3. No record (every Phase 8/10/12 MPS-trained, Mac-packed artifact, and the synthetic fixture the gates use) -> keep today's behaviour (local `laya-verify` is the gate). Do NOT treat absence as pass. This keeps `scripts/laya_gates.tsv` rows `deploy-refusals`, `upload-eligibility`, `verify`, `deploy-selftest-skip` meaningful (they expect `REFUSED eligibility SyntheticNotDeployable` for the fixture). [VERIFIED: scripts/laya_gates.tsv:8,20-22]
4. Honest limit to state in the contract text: the record's authenticity rests on the fetch (manifest sha256 per entry) and the `apr_sha256` equality; a hand-forged `pack-outcome.json` in a local directory is not detectable by the recipe alone. Say so; do not claim more.

### Pattern 4: The D-16 declaration machinery

`laya_repro_eval.py` (pure; stdlib only) validates `laya-reproduction-declaration-v1` via `check-declaration` and computes the verdict via `evaluate --declaration DECL --run-dir DIR --attempt N`. MUST keys are the fixed tuple `deploy_eligible`, `device_prefix`, `recipe_id`, `inputs_sha256` (`task_json`, `train_jsonl`, `eval_jsonl` required), `seeds`; band keys `margin`, `ece_post`; `retry.attempt_cap >= 1`; `retry.retry_classes` may list only `in-band-gate-fail` and `waived` (an empty list is valid); `waiver_rule` is `inconclusive` or `counts`. A pack refusal after a gate pass is class `defect` and never retriable; attempt 1 was exactly that. [VERIFIED: laya_repro_eval.py:1-60 and 117-186]

Consequences for the new declaration:
- `must.recipe_id`, `must.inputs_sha256` and `must.seeds` are compared to what the run's gate report carries. Any D-14 change (data, epochs, eval size) changes the hashes or the recipe id, so the declaration must carry the NEW values, computed before the run (the dataset hashes are known once the data dir exists; `recipe_id` is derived by the trainer, so the planner needs a way to precompute it or the declaration must state it is filled from a dry computation; see Open Question 3).
- Attempt cap 1, `retry_classes: []`, `waiver_rule: "inconclusive"` is the D-12 shape.
- `must_note` should be re-worded for D-08 (the pack task's record is the eligibility; a Mac refusal is a finding).
- Gate-run evidence (`laya-gate-run-evidence-v3` shape in `14-GATE-RUN-EVIDENCE.json`) carries `declaration_sha256`, `declaration_commit {sha, committed_at}`, `verdict`, `next_trusted`, `claim_scope`, `owner_replies`. Use the committed-at timestamp against the training job's start to prove "committed before the paid run".

### Pattern 5: Candidate (k, floor, ceiling) per set against the evidence (the checkpoint brief input)

All numbers below were read from `14-DRIFT-EVIDENCE.json` / `14-GATE-RUN-EVIDENCE.json` with a script this session and recomputed; "headroom" = candidate bound / observed drift. Graviton is the D-04 authority; Mac rows are informational.

**Data points**

| Point | Set | Observed max abs diff (Rust vs torch32 file) | A10G noise | Obs / noise | Bound today | Obs / bound today | Source |
|---|---|---|---|---|---|---|---|
| Graviton, motivation-v1-sm | zero_shot | 1.728534698486328e-05 | 1.7265105816122173e-06 | 10.01 | 1.0e-5 (floor) | 1.7285 REFUSED | rows[8]; GATE-RUN noise.zero_shot |
| Graviton, motivation-v1-sm | fine_tuned | NOT produced; the pack passed this set first, so obs <= 1.3021142293956345e-05 | 3.255285573489086e-06 | <= 4.0 | 1.3021142293956345e-05 | <= 1.0 | verify_loaded order [VERIFIED: verify.rs:3475-3497] + REFUSED line names zero_shot |
| Graviton, smoke-stance-64 | fine_tuned | 2.7477741241455078e-05 | 8.147599085628343e-06 | 3.37 | 3.259039634251337e-05 | 0.8431 accepted | rows[11,13] |
| Graviton, smoke-stance-64 | zero_shot | 1.5079975128173828e-05 | 4.70304053501458e-06 | 3.21 | 1.881216214005832e-05 | 0.8016 accepted | rows[12,14] |
| Mac M4 Pro, smoke-stance-64 (info) | fine_tuned | 3.439188003540039e-05 | 8.147599085628343e-06 | 4.22 | 3.259039634251337e-05 | 1.0553 REFUSED | rows[0] |
| Mac M4 Pro, motivation-v1-sm (info) | zero_shot | 1.4901161193847656e-05 | 1.7265105816122173e-06 | 8.63 | 1.0e-5 | 1.4901 REFUSED | rows[1] |
| Mac, MPS-trained (laya-motivation-v1, laya-stance-64, laya-antisemitism-v1), 6 rows | both | 3.3e-6 to 1.2e-5 | 3.6e-6 to 1.8e-5 | 0.27 to 2.10 | 1.4e-5 to 7.3e-5 | 0.067 to 0.526 | rows[2..7] |

Limits of the claim (state them in the brief): three measured Graviton (set, artifact) cells over two CUDA artifacts; each Graviton leg gave ONE distinct value over 3 repeats (determinism, not host diversity: the Fargate generation is unobservable); eval size differs (100 rows motivation, 459 rows smoke) and spike 028 found a larger eval set makes the max harder (heavy tail) [CITED: .claude/skills/spike-findings-aprender/references/laya-calibration-gate-and-pack-tolerance.md]; x86_64 pack unmeasured; the second set of a refused leg is unmeasured; the motivation fine_tuned Graviton value is only bounded above.

**Candidate grid (illustrative parameter sets, not recommendations)** — `P` = pass, `F` = fail, headroom in parentheses; the unmeasured motivation fine_tuned cell shows its minimum headroom (>=).

| Candidate (ft: k,floor / zs: k,floor) | G motiv zs | G smoke ft | G smoke zs | G motiv ft (>=) | Mac smoke ft (info) | Mac motiv zs (info) |
|---|---|---|---|---|---|---|
| C0 today: 4,1e-5 / 4,1e-5 | F (0.58x) | P (1.19x) | P (1.25x) | P (>=1.00x) | F (0.95x) | F (0.67x) |
| C1: 4,1e-5 / 4,2.5e-5 | P (1.45x) | P (1.19x) | P (1.66x) | P (>=1.00x) | F (0.95x) | P (1.68x) |
| C2: 4,3e-5 / 4,3e-5 | P (1.74x) | P (1.19x) | P (1.99x) | P (>=2.30x) | F (0.95x) | P (2.01x) |
| C3: 4,5e-5 / 4,5e-5 | P (2.89x) | P (1.82x) | P (3.32x) | P (>=3.84x) | P (1.45x) | P (3.36x) |
| C4: 4,1e-5 / 16,1e-5 | P (1.60x) | P (1.19x) | P (4.99x) | P (>=1.00x) | F (0.95x) | P (1.85x) |
| C5: 4,4e-5 / 4,3e-5 | P (1.74x) | P (1.46x) | P (1.99x) | P (>=3.07x) | P (1.16x) | P (2.01x) |
| C6: 12,1e-5 / 12,1e-5 | P (1.20x) | P (3.56x) | P (3.74x) | P (>=3.00x) | P (2.84x) | P (1.39x) |
| C7: 4,1e-4 / 4,1e-4 | P (5.79x) | P (3.64x) | P (6.63x) | P (>=7.68x) | P (2.91x) | P (6.71x) |

Reading it (analysis, not a recommendation):
- Today's only Graviton failure is the zero_shot cell whose bound collapsed to the 1e-5 floor because 4 x 1.7265e-6 = 6.9e-6. Any candidate with `floor_zs` above 1.7285e-5 clears it; C1 is the minimum-blast-radius shape (fine_tuned untouched).
- The Mac smoke fine_tuned point (3.439e-5) only passes when the fine_tuned bound exceeds that; D-04 says it is informational, which is exactly why D-08 makes the Mac advisory instead of tuning the bound to it.
- A multiplier-only fix (C4, C6) is driven by one cell (10.01x) and scales slack with each checkpoint's own noise: with the worst noise measured on any checkpoint, 6.27e-5 (spike 028) [CITED: contracts/laya-parity-v1.yaml:99-104], k=12 yields 7.5e-4 and k=16 yields 1.003e-3, which the current ceiling would refuse. An absolute floor gives constant slack.
- Why absolute slack is the better-argued shape (analysis): the noise term is torch-on-A10G vs float64 on the SAME device; the re-score compares Rust-on-Graviton to the A10G torch file, so the gap contains a CPU-vs-CUDA term the record cannot see. A1's k=4 was fitted with torch and Rust both on aarch64 (shared rounding walks; k_needed peaked at 1.18; Rust error / torch error 0.24 to 1.59) [CITED: contracts/laya-parity-v1.yaml:48-65]. The three Graviton points show absolute drift in a narrow 1.5e-5 to 2.75e-5 band while obs/noise ranges 3.2x to 10.0x. MPS-trained artifacts show obs/noise below 2.1 because MPS noise is itself large. [ASSUMED: that the CUDA-vs-CPU term is the explanation; the data fit it, but no measurement isolates it]
- The ceiling only ever refuses (it bounds a derived bound, i.e. forged or ill-conditioned records); it is not the regression detector. The regression detector is the bound itself. Today's ceiling 1.0e-3 was chosen as 4x the worst measured noise-derived bound (2.5e-4 = 4 x 6.27e-5) [CITED: contracts/laya-parity-v1.yaml:99-104]. A per-set ceiling can stay 1.0e-3 or tighten; tightening is only meaningful if a per-set k multiplies noise.

**The red line (D-06), what is on record:** the smallest cited porting regression is the local-window off-by-one, |dp| 1.4e-2 (window 63) and 2.8e-2 (window 65, flips an argmax) [CITED: contracts/laya-parity-v1.yaml window_mutation invariants, spike 025]. That is 140x the largest candidate floor in the grid (1e-4, C7) and 560x the smallest enforced floor that clears every Graviton cell (2.5e-5, C1). There is no measured regression between 1e-4 and 1e-2, so the checkpoint should state: bound <= X is argued to be well below the smallest known regression class and argmax stays EXACT (unchanged, 0 flips over 9,708 row scorings in spike 028) [CITED: contracts/laya-parity-v1.yaml:189]; whether anything between the bound and 1.4e-2 would be caught is unmeasured. [ASSUMED: no sub-1e-2 regression of interest exists; the owner should know this is not shown]

### Anti-Patterns to Avoid
- **Editing `k` or `float_tolerance` in place.** `check_noise_header` compares them bit for bit with the record header (and with `contract.noise_policy()` in Python); changing either invalidates every stored record, including the retained motivation-v1-sm run needed for SC3.
- **Per-set floor applied to the absent-record path.** It would loosen the "no record" check and break `fail_closed_vectors` (2 vectors expect refusal at the 1e-5 floor).
- **Demoting every Mac refusal to advisory.** Only `RescoreDrift` is a device-float-noise outcome; load-ladder, manifest, gate, base and `SyntheticNotDeployable` refusals must still block.
- **Treating a missing `pack-outcome.json` as pass.** Mac-packed MPS artifacts and the synthetic fixture have none; fall back to today's local gate.
- **Writing Phase 15 evidence into `14-DRIFT-EVIDENCE.json`.** `laya_drift.py collect` merges into it and recomputes `status`; use a new `15-*` evidence file and leave Phase 14 records equal to HEAD.
- **Closing a guard with a case table that was never mutated.** CLAUDE.md Verification Discipline 4 and 7: show each new refusal turn RED under a mutation.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Contract validation / version suggestion | a shell/yq/python checker | `./target/release/pv validate|lint|diff` (`pv diff` takes two file paths; `git show REV:path > /tmp/old.yaml`) | CLAUDE.md "Never work around `pv` with a shell script" |
| D-16 verdict | a new evaluator | `scripts/laya_deploy/laya_repro_eval.py check-declaration|evaluate` | pure, selftested, same one used by 14-14/14-15 |
| Fetching a finished run | ad-hoc `aws s3 cp` | `just laya-fetch` (`laya_fetch.py`) | sha256-verified against the manifest, closed path mapping |
| Owner-run Graviton re-pack script | a hand-written `aws ecs run-task` script | `laya_drift.py aws-plan --source motivation:<TASK_ID>` + `bashrs-gate` | generates the script with the account-check, tag read-back and expire tags; static policy table already judges it |
| Image build / stack deploy | manual docker push / cdk | `just laya-trainer-image pack`, `just laya-trainer-infra diff|deploy` | provenance, digest recording and `cdk diff` before `deploy` are built in |
| sha256 of the `.apr` | hand-rolled hashing | `shasum -a 256` (as the recipes already do) | recipes already use it |
| Bound derivation | a second copy in Python or the recipe | the Rust `rescore_bounds` only (OPS-03, one implementation) | the trainer only records; Rust derives and enforces |

**Key insight:** every piece of machinery this phase needs already exists and is gated; the work is a careful contract+verifier edit, a recipe edit, an image roll, and two owner-run paid steps.

## Runtime State Inventory

Not a rename phase, but the amendment has deployed copies of the contract that a repo edit does not update.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | Retained CUDA run `motivation-v1-sm` in S3 (`runs/.../a2cbacae-529c-4703-a796-b6eda6742446`, tagged `laya-retention=expire`, expires about 2026-11-28) and a local gitignored copy under `models/decide/motivation-v1-sm`; its `rescore-noise.json` is a v1 record (header k 4.0, floor_abs 1e-5) | Re-pack before the expiry; do not change the record schema (Option S-B keeps it valid) |
| Live service config | Pack task definition pins an image digest (the drift script reads it with `ecs describe-task-definition`); the finalizer Lambda code is built from a closure that includes `aprender-decide`; CDK stacks `aprender-laya-trainer-images` / `aprender-laya-trainer` | Owner-run image build, then `infra diff` (Claude reads) then `infra deploy`; diff should show a pack TaskDefinition image change and the Finalizer code key [ASSUMED until the diff is generated; STOP on any IAM statement change, per the 14-22 runbook] |
| OS-registered state | None found: no launchd/Task Scheduler/pm2 entries reference the bound (searched repo scripts and justfile; machine-level registrations not inspected) | none |
| Secrets/env vars | `LAYA_M2M_FILE`, `LAYA_DRIFT_APPROVED=graviton`, `LAYA_TRAINER_APPROVED_SPEND`, profile `ze-kasher-dev`; no secret is named after the bound | none; keep account ids and ARNs out of committed evidence (`laya_drift.py` scrubs 12-digit runs; IN-09 shows what happens otherwise) |
| Build artifacts / installed packages | Pack image (contract + compiled `pack_laya` baked in), train image (contract baked, harmless under S-B), `deploy-extensions/assets/laya-finalizer/bootstrap` + `bootstrap.build.json` sidecar (gitignored, provenance-checked), `target/release/examples/pack_laya` | Rebuild pack image; re-synth the finalizer asset (provenance refuses a stale one); rebuild local `pack_laya` (cargo does) |

Served decide servers (pmcp.run) load `.apr` artifacts and replay probes at `probe_probabilities_abs`; they do not read the pack bound, so no server redeploy is implied.

## Common Pitfalls

### Pitfall 1: A "per-set" edit that invalidates every stored record
**What goes wrong:** `RescoreNoiseInvalid field=k|floor_abs` or `RescoreNoiseMismatch field=bound` on the motivation-v1-sm re-pack, and on any run written by the current trainer.
**Why:** header and per-set `bound` are cross-checked bit for bit against the contract (verify.rs:2236-2260, 2426-2434).
**Avoid:** Option S-B; add a test that a v1 record written with the stamp values verifies under the amended policy and that a record stamped with the wrong k is still refused.
**Warning signs:** `noise_k_mismatch`, `forged_noise_value` tests changing meaning; any diff to `scripts/laya_train/`.

### Pitfall 2: The amended bound never reaches Graviton
**What goes wrong:** the re-pack runs on the old image and refuses at 1e-5 again; the paid run then fails the same way.
**Why:** the contract is baked into the image (`COPY contracts/` and a `pack_laya` compiled at `/src/crates/aprender-decide`); the task definition pins a digest.
**Avoid:** the re-pack's evidence must show the NEW `pack_image_digest` (in `pack-outcome.json`) and that its `parity_contract_sha256` equals `shasum -a 256 contracts/laya-parity-v1.yaml` of the committed amended file. Cite digest + hash, not intent (Verification Discipline 2).

### Pitfall 3: Finalizer provenance refuses `infra deploy`
**What goes wrong:** `REFUSED infra: finalizer provenance: closure path changed since the build`.
**Why:** `aprender-decide` is in the finalizer's `cargo tree` closure [VERIFIED: `cargo tree -p aprender-mcp-laya-train-lambda -e normal` lists `aprender-decide v0.69.3`]; any commit touching it after the synth invalidates the sidecar.
**Avoid:** commit all decide edits first, then `just laya-trainer-synth dev`, then diff/deploy without further commits to the closure (planning-file commits are outside the closure and safe). Budget the finalizer code-key change in the diff review.

### Pitfall 4: D-14 asks for levers the gate contract pins
**What goes wrong:** a declaration that changes learning rate, batch size, warmup or the seed set produces a run the Rust verifier refuses (`RecipeMismatch`, `SeedPolicyMismatch`) or a recipe the trainer will not accept.
**Why:** `laya-finetune-gate-v1.yaml` `recipe:` pins `encoder_lr 2.5e-5`, `head_lr 1.0e-4`, `batch_size 8`, `eta_min`, `weight_decay`, `grad_clip`, cosine schedule (no warmup exists) and states "Any change is a new recipe_id"; `check_recipe_block` compares each (verify.rs:1534-1580); `seed_policy.variance_seeds [13, 17, 23]` with `production_seeds_required: 3` and `declared_seed: 13` are checked by `check_seed_decl`; epochs are only free inside `[epochs_above_16_min 4, epochs_above_16_max 12]` at >16 shots per class (188 for motivation-v1). [VERIFIED: contracts/laya-finetune-gate-v1.yaml:165-296 read this session, verify.rs:1534-1580]
**Avoid:** within existing contracts the live levers are (a) max epochs in [4, 12] (attempt 1 used 12; early stopping already ships the best calibration-NLL epoch, so lowering the maximum only helps if a later epoch hurts), (b) the training/eval data (a new data dir, hashes and a NEW dataset prefix: prefixes are immutable and `stage dataset --expect motivation-v1` pins Phase 12's hashes, so use it without `--expect` or add a pin), (c) eval-set size only through `prepare_motivation.py` (`EVAL_DIVISOR = 4` is a rule constant with a selftest, i.e. a code change). Learning rate, batch size, warmup and the seed set require editing `laya-finetune-gate-v1.yaml`, a second one-way contract edit that D-14 did not scope; surface this to the owner at the declaration checkpoint instead of silently planning it.

### Pitfall 5: Bigger eval set quietly raises the drift
**What goes wrong:** D-14 permits growing the eval set to reduce per-seed variance; the max-of-heavy-tail drift grows with rows (spike 028: larger eval set makes the bar harder; smoke 459 rows drifted 2.75e-5 fine_tuned on Graviton vs motivation's 100 rows). The chosen bound must be argued for the eval size the declaration will actually use.
**Avoid:** put the eval size next to the bound at the checkpoint; prefer headroom >= the grid's 1.5x on the largest observed (2.75e-5) if the eval set grows.

### Pitfall 6: One attempt, and a post-gate pack refusal consumes it
**What goes wrong:** D-12 cap 1; attempt 1 ended `defect` because the pack refused after the gate passed. A refusal on the single new attempt ends D-16 without a retry.
**Avoid:** run the D-15 re-pack first (it exercises the amended bound on a real CUDA record on Graviton); note that if the eval rows are unchanged the zero_shot drift is probably the same 1.7285e-5 [ASSUMED: CUDA zero-shot probabilities reproduce across runs], while the new checkpoint's fine_tuned drift is a fresh draw (upper-bounded only by what was seen: 2.75e-5 on 459 rows, <=1.3e-5 on 100 rows). Choose floors with that draw in mind; recorded as a risk for the owner.

### Pitfall 7: Landing in band is not guaranteed
**What goes wrong:** attempt 1 per-seed margins were 0.3400 (seed 13), 0.2860 (seed 17), 0.3541 (seed 23) with ECE 0.1124, 0.0964, 0.0767; the median-ECE seed (17) ships and had the lowest margin. At 100 eval rows this is a wide spread, and the ECE gate (<= 0.10) failed for seed 13. [VERIFIED: 14-GATE-RUN-EVIDENCE.json seeds.per_seed via python]
**Avoid:** state to the owner that the amendment only removes the pack-refusal cause; a miss on margin is still possible. Phase 12 (MPS) shipped 0.378. Effective ECE target is [0.07, 0.10] because the gate requires <= 0.10 inside a [0.07, 0.12] band.

### Pitfall 8: Selecting by tests that pin literals
**What goes wrong:** tests that compute expected bounds from `policy.rescore_noise_k` / `policy.rescore_probs_abs` (verify/tests.rs lines ~1677, 1703, 1794, 1836, 1902, 1982) silently test the wrong quantity after the policy grows per-set fields.
**Avoid:** keep `write_noise_record`'s stamp from the stamp fields, compute the enforced bound from the new per-set fields, and add a cell per set where the two differ so a swapped field turns a test red.

### Pitfall 9: Tooling traps on this box
`pv` is not on PATH; `rg` is aliased to grep through the rtk hook (`--glob` fails); `diff` exit status is unreliable through the hook (use `cmp`); `ci_run_explicit_test_commands.sh --list` needs GNU find (macOS reports the dir empty); `pv lint contracts/` (directory form) FAILS at HEAD at Gate 10 `sigma` (2 errors) in this working tree while the file form `pv lint contracts/laya-parity-v1.yaml` PASSES, so judge the amendment by the file form plus a before/after comparison, or run the directory form on a clean export as `laya-gap-regression` does. [VERIFIED: ran both this session]

## Code Examples

Sketches only; placeholders `<...>` are the owner's checkpoint values, not proposals.

### Contract keys (Option S-B), flat scalars
```yaml
# constants: (A1 keys unchanged and re-described as the record STAMP + absent-record floor)
pack_rescore_noise_k: 4                          # stamp: record header k and per-set reported bound
pack_rescore_bound_max_abs: 1.0e-3               # stamp-level ceiling, unchanged
# A2 (per-set, enforced when a rescore-noise.json record exists):
pack_rescore_noise_k_fine_tuned: <K_FT>
pack_rescore_floor_fine_tuned: <FLOOR_FT>        # must be >= equations.pack_rescore_probs_abs.float_tolerance
pack_rescore_bound_max_abs_fine_tuned: <CEIL_FT>
pack_rescore_noise_k_zero_shot: <K_ZS>
pack_rescore_floor_zero_shot: <FLOOR_ZS>
pack_rescore_bound_max_abs_zero_shot: <CEIL_ZS>
```

### Verifier shape (one implementation, no `unwrap`)
```rust
// Source: crates/aprender-decide/src/verify.rs rescore_bounds (read this session); sketch of the change.
// header + reported-bound cross-checks keep using the STAMP (policy.rescore_noise_k, policy.rescore_probs_abs);
// the enforced bound per set uses the A2 per-set fields.
let stamp_bound = floor.max(policy.rescore_noise_k * noise);
if set.bound.to_bits() != stamp_bound.to_bits() { /* RescoreNoiseMismatch, unchanged */ }
let p = policy.rescore_set(which);                 // per-set (k, floor, ceiling)
let bound = p.floor.max(p.k * noise);              // >= policy.rescore_probs_abs by contract
if !within(bound, p.ceiling) { /* RescoreBoundCeiling { which, bound, ceiling: p.ceiling } */ }
```
Add a load-time invariant (`from_contract_views` or a test): `floor_s >= float_tolerance`, `ceiling_s >= floor_s`, all finite, so "a record can only loosen relative to no record, never below the floor" holds by construction.

### Recipe gate (laya-deploy step 5 / laya-upload), shape
```bash
# Source: justfile laya-deploy step (5) (read this session); shape of the D-08/D-09 change.
REC="${LAYA_PACK_OUTCOME:-$(dirname "{{run}}")/logs/pack-outcome.json}"
# 1) if "$REC" exists: python3 -I - "$REC" "$H" must see schema laya-train-pack-outcome-v1,
#    accepted is True, deploy_eligible is True, apr_sha256 is a 64-hex string equal to $H  (else REFUSED eligibility).
# 2) run `just laya-verify ...` as today; on non-zero: if the first REFUSED line starts "REFUSED RescoreDrift"
#    AND (1) passed => append a "CROSS-CPU FINDING" line to "$SD/cross-cpu-finding-$SERVER.log" and continue;
#    any other refusal => refuse as today.
# 3) no "$REC" => today's behaviour unchanged.
```

### Declaration skeleton (committed before the paid run)
```json
{
  "schema": "laya-reproduction-declaration-v1",
  "plan": "15-NN", "declared_on": "<date>",
  "must": { "deploy_eligible": true, "device_prefix": "cuda", "recipe_id": "<computed>",
            "inputs_sha256": {"task_json": "<sha>", "train_jsonl": "<sha>", "eval_jsonl": "<sha>"},
            "seeds": [13, 17, 23] },
  "must_note": "<D-08 wording: deploy_eligible is the pack task's record; a Mac laya-verify refusal is a recorded cross-CPU finding>",
  "band": { "margin": [0.3, 0.45], "ece_post": [0.07, 0.12] },
  "retry": { "attempt_cap": 1, "retry_classes": [], "rule": "..." },
  "waiver_rule": "inconclusive",
  "changes_from_attempt_1": ["<every D-14 change with concrete values>"],
  "claim_scope": "not an identical reproduction of attempt 1 or of Phase 12; <wording>"
}
```
(`laya_repro_eval.py check-declaration` must print `DECLARATION OK`; confirm which extra keys it tolerates before adding `changes_from_attempt_1`/`claim_scope`; see Open Question 4.)

## Owner-run AWS command sequence (to stage with `!`)

Derived from the committed recipes and the 14-22 runbook pattern; none were executed here. Each is owner-run; Claude reads the files they write and the orchestrator records replies.

| # | Step | Command shape | Output to judge | Cost |
|---|---|---|---|---|
| 0 | prerequisites (Claude, offline) | commit contract+Rust+justfile; `cargo test`, `pv`, selftests green; clean closure | green logs | none |
| 1 | build pack image | `! assume ze-kasher-dev && just laya-trainer-image pack dev ze-kasher-dev` (background; up to 90 min; refuses if a context path is dirty) | last line `IMAGE OK pack <digest>` | CodeBuild minutes |
| 2 | re-synth finalizer (Claude, offline) | `just laya-trainer-synth dev` after the commit; then `finalizer-provenance verify` | `TRAINER SYNTH OK` | none |
| 3 | stack diff | `! assume ze-kasher-dev && just laya-trainer-infra diff dev ze-kasher-dev > models/laya-trainer/<f>.txt 2>&1; tail -n 2 <f>` | Claude reads: expected pack TaskDefinition image + Finalizer code key; STOP on IAM change or `[+]/[-]` | none |
| 4 | stack deploy | `! assume ze-kasher-dev && just laya-trainer-infra deploy dev ze-kasher-dev` | `INFRA OK deploy` | ~1 min |
| 5 | D-15 re-pack | `python3 -I scripts/laya_deploy/laya_drift.py aws-plan --source motivation:a2cbacae-529c-4703-a796-b6eda6742446` (writes the gitignored script; `bashrs-gate` it) then `! assume ze-kasher-dev && LAYA_DRIFT_APPROVED=graviton bash models/laya-trainer/drift-graviton.sh motivation:1` | `DRIFT GRAVITON DONE motivation:1`; `pack-outcome.json` accepted true, deploy_eligible true, `pack_image_digest` = step 1 digest | ~2 min Fargate |
| 6 | (if data changed) stage dataset | `! assume ze-kasher-dev && just laya-trainer-stage dataset DIR PREFIX dev ze-kasher-dev` (new immutable prefix; no `--expect motivation-v1` unless a new pin is added) | hashes in `datasets-dev.json` | S3 |
| 7 | the one paid attempt | `just laya-trainer-call ze-kasher-dev URL train URI NAME [EPOCHS]` with `LAYA_M2M_FILE`, `LAYA_TRAINER_APPROVED_SPEND` as the recipe requires; poll `status` | task completes; | attempt 1: 1346 s ml.g5.xlarge + 124 s Fargate |
| 8 | fetch + evaluate | `just laya-fetch NAME TASK_ID BUCKET` then `python3 -I scripts/laya_deploy/laya_repro_eval.py evaluate --declaration <15-decl> --run-dir models/decide/NAME --attempt 1` | one JSON line: class, verdict_if_final | none |

Operational notes: aws-cli 2.29.0 prints `None` for `KeyCount` on an empty prefix (a false `REFUSED prefix` once; fixed in the generated script), and `drift-graviton.sh` is gitignored because it holds the account id. Hygiene after each run (as attempt 1 recorded): training jobs InProgress 0, ECS tasks running 0, state machine SUCCEEDED, single-flight lock released.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed 1e-5 on real rows (spike 025 fixtures) | A1 noise-referenced `max(1e-5, 4 x noise)` | 2026-09-27 (plan 08-13) | fixed a refusal for torch's own rounding when torch and Rust shared one CPU |
| Local `laya-verify` as the deploy gate | pack task record as the gate; Mac advisory | this phase (D-08) | removes a cross-CPU false refusal from the deploy path |

**Deprecated/outdated:** the CLAUDE.md sentence that the bar is `max(1e-5, 4 x ...)` becomes incomplete after A2; the Phase 14 declaration's `attempt_cap: 2` does not carry over.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The absolute drift is stable (1.5e-5 to 2.75e-5) because the dominant term is the CUDA-vs-CPU gap that the A10G noise record cannot see | Pattern 5 | If wrong, an absolute floor over-loosens and a multiplier would have been the better shape; mitigated by showing both in the grid |
| A2 | No porting regression of interest lies between the largest candidate bound (<=1e-4) and the smallest cited regression (1.4e-2) | Pattern 5 red line | A subtle kernel/accumulation regression in that gap would pass; D-06 asks the plan to state the red line, so the owner should see this as an unmeasured region |
| A3 | CUDA zero-shot probabilities on unchanged eval rows reproduce across runs, so the zero_shot Graviton drift of a new run on the same eval set is about 1.7285e-5 again | Pitfall 6 | If wrong, the re-pack is a weaker predictor of the D-16 pack outcome |
| A4 | The `infra diff` after the pack-image rollout shows only a pack TaskDefinition image change and the Finalizer code key | Runtime State Inventory | If it shows IAM/resource changes, STOP per the runbook; plan a decision point |
| A5 | `laya_repro_eval.py check-declaration` tolerates extra informational keys (`changes_from_attempt_1`, `claim_scope`) | Code Examples | If not, record those in the evidence file instead of the declaration |
| A6 | The re-pack of motivation-v1-sm through the drift script's `runs/drift-motivation/...` path counts as the "Graviton pack task" for SC3 (pack-outcome.json is the same record type) | Command sequence | If the owner wants the real pipeline path, a different mechanism is needed (none exists for re-running a finished run) |
| A7 | Lowering max epochs below 12 does not by itself change early-stopping outcomes materially | Pitfall 4 | A candidate training change that does not move the result wastes the single attempt |

## Open Questions

1. **Structure choice at the checkpoint (S-B vs S-A).**
   - Known: S-B keeps every stored record valid and the trainer untouched; S-A needs a record v2 and a train-image rebuild and endangers the retained run.
   - Unclear: the owner has not seen the structure, only "per set".
   - Recommendation: put S-B in the checkpoint brief as the default with S-A listed and rejected with reasons.

2. **Should only `RescoreDrift` be demoted on the Mac, and what about artifacts with no pack record?**
   - Known: D-08 says "a Mac refusal is logged as a cross-CPU finding"; MPS-trained and fixture artifacts have no record.
   - Recommendation: demote only `RescoreDrift` when a valid record exists; keep today's gate when no record exists. Ask the owner to confirm in the same sign-off.

3. **How does the declaration obtain `recipe_id` before the run?**
   - Known: `must.recipe_id` is compared to the run's gate report; Phase 12/14 had it from a prior run. A changed epochs/data value changes it.
   - Unclear: whether a dry computation path exists in `scripts/laya_train` (the trainer derives it).
   - Recommendation: planner to locate the derivation (recipe canonicalisation in `train.py`/`data.py`) and compute it offline from the new recipe.json text, or declare it from a local dry run, before the declaration is committed.

4. **Which D-14 levers does the owner actually approve, given the gate contract pins most of them?**
   - Known: within current contracts only max epochs [4, 12], training/eval data and (via code) eval size are movable; lr, batch size, warmup and the seed set need a `laya-finetune-gate-v1` edit.
   - Recommendation: the declaration checkpoint should offer only the movable levers by default and state the contract-edit cost of the rest; no gate-contract edit without its own owner decision.

5. **Does the re-pack need the `.apr` fetched back?**
   - Known: `pack-outcome.json` (accepted, deploy_eligible, apr_sha256) satisfies SC3; fetching the 807 MB `.apr` is only needed to exercise D-09 in a `DRY_RUN` of `laya-deploy`.
   - Recommendation: fetch only if the plan wants an end-to-end D-09 demonstration; otherwise cover D-09 by selftest cases.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc | verifier edit, tests | yes | 1.93.0 | none |
| just | recipes, selftests | yes | 1.46.0 | none |
| `pv` | contract validate/lint/diff | yes (not on PATH) | 0.69.3 at `./target/release/pv` | `cargo run --release -q -p aprender-contracts-cli --bin pv --` |
| uv + Python | `just laya-train-selftest`, evaluator | yes | uv 0.9.5, Python 3.14.6 | none |
| bashrs | gate for generated owner scripts | yes | 6.66.3 | none |
| pmat | code search | yes | 3.15.0 (CLAUDE.md says 3.30.0; drift in the doc, not a blocker) | grep for YAML/justfile keys |
| aws-cli / cdk / node / docker | OWNER-run chain only | yes | aws-cli 2.29.0, cdk 2.1146.1, node 24.19.0, docker 29.9.1 | executor must NOT run aws |
| SageMaker ml.g5.xlarge quota, Fargate arm64, CodeBuild | paid steps | not probed (no AWS call allowed) | attempt 1 ran on them 2026-10-29 | owner confirms at the compute go |
| Real Laya base snapshot (`LAYA_MODEL_DIR`) | `laya-verify-suite`, `laya-gap-regression`, armed `fail_closed_vectors` | not probed | — | those legs SKIP without it; the plan should say which are armed |

**Missing dependencies with no fallback:** none found offline.

## Validation Architecture

`workflow.nyquist_validation` is true in `.planning/config.json` [VERIFIED: config read this session].

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust libtest (`cargo test`), Python stdlib selftests (`laya_repro_eval.py --selftest`, `laya_fetch.py --selftest`, `laya_drift.py --selftest`, `just laya-train-selftest`), justfile selftests (`just laya-deploy-selftest`, `just laya-gates-selftest`), `pv` |
| Config file | `rust-toolchain.toml`, `scripts/laya_gates.tsv`, `scripts/laya_claims.tsv`, `contracts/aprender/binding.yaml` |
| Quick run command | `cargo test -p aprender-decide --lib verify::tests::noise` (7 tests, 18 s incl. compile, baseline green this session) |
| Full suite command | `cargo test -p aprender-decide` + `just laya-train-selftest` + `just laya-deploy-selftest` + `python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest` + `./target/release/pv validate contracts/laya-parity-v1.yaml` + `just laya-claims-check`; the heavy class sweep is `lockf -k /tmp/aprender-laya-real-weights.lock just laya-gap-regression` (real weights, run on the main checkout) |

### Success Criteria -> Verification Map
| Criterion | Behavior | Test Type | Automated Command / Evidence | File Exists? |
|-----------|----------|-----------|------------------------------|--------------|
| SC1 owner sign-off precedes the edit | decision record on disk (owner reply verbatim, parameters, structure, laya-verify role) committed BEFORE the first commit that touches `contracts/laya-parity-v1.yaml` | process/git-order | a small check: `git log --format=%H --diff-filter=... -- contracts/laya-parity-v1.yaml` earliest Phase 15 commit is a descendant of the decision-record commit (`git merge-base --is-ancestor`) | Wave 0 (new decision file + check) |
| SC1 per-set bound amended and valid | `pv validate` 0 errors; `pv lint` file form PASS; version bumped consistently with `pv diff` of `git show HEAD:contracts/laya-parity-v1.yaml` vs the new file | contract | `./target/release/pv validate contracts/laya-parity-v1.yaml`; `./target/release/pv lint contracts/laya-parity-v1.yaml`; `pv diff /tmp/old.yaml contracts/laya-parity-v1.yaml` | contract exists; check new |
| SC1 per-set enforcement in Rust | each data-point row passes/fails per the chosen params (table-driven from the 14-DRIFT-EVIDENCE numbers); a v1 record verifies; wrong stamp k refused; per-set ceiling refused; absent record still 1e-5 for both sets; each guard shown RED under mutation | unit | `cargo test -p aprender-decide --lib verify::tests::` (new `noise_per_set_*` tests) | Wave 0 |
| SC1 no regression of fail-closed vectors | no-record vectors still refused at 1e-5 | integration (armed) | `LAYA_FAIL_CLOSED_VECTORS=1 LAYA_MODEL_DIR=... cargo test -p aprender-decide --release --test fail_closed_vectors -- --nocapture` (needs the base snapshot; says `SKIP` otherwise, never count a SKIP as pass) | exists (armed) |
| SC2 role of `laya-verify` stated and implemented | contract text names the role; recipes honour pack record + sha; cases: record ok + Mac RescoreDrift => proceeds with finding logged; record ok + other refusal => REFUSED; record sha mismatch => REFUSED; record accepted false => REFUSED; no record => unchanged | recipe selftest | extend `just laya-deploy-selftest` and the `scripts/laya_gates.tsv` rows `deploy-refusals`/`upload-eligibility` with a must-match / must-not-match case table (Discipline 7), each mutated RED | Wave 0 |
| SC3 accepted deploy-eligible `.apr` from Graviton under the amended bound | `pack-outcome.json` of the D-15 re-pack: `accepted` true, `deploy_eligible` true, `apr_sha256` 64-hex, `pack_image_digest` equals the digest built from the amended commit, `parity_contract_sha256` equals the committed amended contract's sha256 | evidence check | a read-only script over the fetched/collected `pack-outcome.json` -> `15-REPACK-EVIDENCE.json` (new file, not `14-DRIFT-EVIDENCE.json`) | Wave 0 |
| SC4 D-16 in band, accepted, deploy-eligible, no waiver | new declaration passes `check-declaration`; `evaluate` prints class `pass`, `verdict_if_final` `reproduced`; evidence has `next_trusted` true, `pilot_waiver` null, no `overrides:` entry in any Phase 15 verification file | evaluator | `python3 -I scripts/laya_deploy/laya_repro_eval.py check-declaration 15-REPRODUCTION-DECLARATION.json` ; `... evaluate --declaration ... --run-dir models/decide/<name> --attempt 1` | script exists; declaration Wave 0 |
| SC4 declared before the run | `declaration_commit.committed_at` precedes the training job start; `declaration_sha256` matches the file | evidence check | in `15-GATE-RUN-EVIDENCE.json` (same shape as `14-GATE-RUN-EVIDENCE.json`) | Wave 0 |
| SC5 owner-run AWS only | no aws/cdk/pmcp command run by an executor; every paid run has an owner reply recorded by the orchestrator | process | owner-reply records in the phase dir; generated scripts pass `bashrs-gate`; DRY_RUN prints `AWS CALLS: 0` for staged recipes | process |
| Docs coherence | CLAUDE.md bar sentence updated; every path it cites exists | gate | `cargo test -p aprender-core --test readme_contract` | exists |

### Sampling Rate
- **Per task commit:** `cargo test -p aprender-decide --lib verify::tests::noise` plus any new `noise_per_set_*`; `./target/release/pv validate contracts/laya-parity-v1.yaml` when the contract changes; `cargo fmt --all -- --check`; `cargo clippy -p aprender-decide --lib -- -D warnings`.
- **Per wave merge:** `cargo test -p aprender-decide`, `just laya-deploy-selftest`, `just laya-claims-check`, `python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest`, `cargo test -p aprender-core --test readme_contract`.
- **Phase gate:** the above plus `just laya-train-selftest` (proves the untouched trainer still writes a record the amended verifier accepts: `tests/python_records.rs` leg), and `just laya-gap-regression` (or the documented subset on the main checkout) before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `15-BOUND-DECISION.*` record + a git-order check for SC1 (new)
- [ ] `noise_per_set_*` Rust tests incl. v1-record-still-valid, per-set ceiling, absent-record floor, swapped-field mutation (new, in `crates/aprender-decide/src/verify/tests.rs`)
- [ ] recipe case table for the D-08/D-09 gate in `laya-deploy-selftest` and `scripts/laya_gates.tsv` rows (new)
- [ ] `15-REPACK-EVIDENCE.json` producer/check (new; do not reuse `laya_drift.py collect` output file)
- [ ] `15-REPRODUCTION-DECLARATION.json` + `check-declaration` run (new)
- [ ] baseline note: `pv lint contracts/` (directory form) fails Gate 10 at HEAD in this working tree, so record the baseline and compare, or use the clean export

## Security Domain

`security_enforcement` is enabled (absent = enabled; config has `security_enforcement: true`, ASVS level 1, block on high).

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (no new auth surface; M2M token handling is existing `m2m_token.py`) | existing: token via env only, never argv |
| V3 Session Management | no | — |
| V4 Access Control | yes (owner-run gating) | profile `ze-kasher-dev` only; `LAYA_*_APPROVED` env gates; no executor AWS calls |
| V5 Input Validation | yes | strict JSON parse of `pack-outcome.json` (python `json`, schema string, 64-hex regex for `apr_sha256`, booleans compared with `is True`); Rust serde views; reject NaN/inf in contract numbers (`contract.number` / `within` NaN-visible idiom) |
| V6 Cryptography | yes (integrity only) | `shasum -a 256` / sha2 already used; never hand-roll; compare full 64-hex |
| V7 Error handling / logging | yes | cross-CPU finding logs must not contain account ids/ARNs; committed evidence scrubbed (12-digit runs, ARNs) as `laya_drift.py` does (IN-09 is the known failure) |
| V14 Configuration | yes | image digest pinned in `pack_image_digest`; contract sha256 recorded in `pack-outcome.json` |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Loosening the bound by omitting or editing the noise record | Tampering | record is hash-bound by `rescore_noise_sha256`; Rust recomputes noise and bound; absent record = 1e-5 floor for both sets (kept); ceiling refuses inflated bounds |
| A per-set floor chosen to wave through a regression | Tampering/Elevation | red line stated against the smallest cited regression (1.4e-2); argmax stays exact; owner sign-off before edit; contract change recorded as an amendment with falsification test |
| Forged local `pack-outcome.json` makes the Mac skip the Rust check | Spoofing | bind `apr_sha256` to the file's sha256 (D-09); fetched files are manifest-hash-verified; demote only `RescoreDrift`, keep all other refusals; state the residual risk in the contract text |
| Verdict not tied to the bytes deployed (TOCTOU) | Tampering | sha256 equality between `pack-outcome.json` `apr_sha256`, the local file and the deploy config's `APRENDER_DECIDE_SHA256` pin (already enforced by step (3)) |
| Stale image used for the paid run | Repudiation | require `pack_image_digest` and `parity_contract_sha256` in the evidence to match the build from the amended commit |
| Secrets/account ids in committed evidence | Information disclosure | scrub rule, no `.env`/token files committed (`.env.m2m` is currently untracked in the working tree; do not add it) |
| Paid run started without approval | Repudiation | owner-recorded compute go before step 7; attempt cap 1 declared in the committed declaration |

## Sources

### Primary (HIGH confidence; opened or ran this session)
- `contracts/laya-parity-v1.yaml` (read in full): A1 text lines 48-65, constants lines 87-104, `pack_rescore_probs_abs` lines 179-193, `rescore_noise_reference` lines 195-205, FALSIFY-005/006 lines 273-286, qa_gate lines 305-322
- `crates/aprender-decide/src/verify.rs` (policy 96-114, views 470-486, `from_contract_views` ~500-515, `check_noise_header` 2225-2275, `rescore_bounds` 2394-2449, `verify_loaded` 3467-3500, `check_recipe_block` 1534-1580); `src/pack.rs` `RescoreNoise` 380-420; `tests/fail_closed_vectors.rs`; `examples/pack_laya.rs`
- `scripts/laya_train/contract.py` (`noise_policy`), `train.py` (`write_noise_record`), `data.py` (selftest mutation), `container/run_step.py` (`new_pack_outcome`, pack/verify/upload block), `container/Dockerfile.train`, `container/context-paths.txt`
- `deploy-extensions/container/pack-arm64/Dockerfile` and `context-paths.txt`
- `justfile` (`laya-verify` 1765, `laya-deploy` 2410-2540, `laya-upload` ~4056-4125, trainer recipes 1390-1540, `laya-gap-regression` ~6570+)
- `scripts/laya_deploy/laya_repro_eval.py` (docstring + declaration check), `laya_fetch.py` (`dest_for`), `laya_drift.py` (docstring, aws-plan), `scripts/laya_gates.tsv`, `scripts/laya_claims.tsv`
- `.planning/phases/14-*/14-DRIFT-EVIDENCE.json` (rows, a10g_noise, graviton_spread via python), `14-GATE-RUN-EVIDENCE.json`, `14-REPRODUCTION-DECLARATION.json`, `14-DRIFT-DECISION-BRIEF.md`, `14-GAP-ROUND-2-OWNER-DECISIONS.md`, `14-REDEPLOY-RUNBOOK.md`, `14-15-SUMMARY.md`
- `contracts/laya-finetune-gate-v1.yaml` lines 165-330 (constants, seed_policy, eval_set, pilot_waivers, motivation_v1, early_stopping) and schema blocks ~535-575, 770-800
- `.claude/skills/spike-findings-aprender/references/laya-calibration-gate-and-pack-tolerance.md`
- Commands run: `cargo test -p aprender-decide --lib verify::tests::noise` (7 passed), `./target/release/pv validate|lint|diff --help`, `pv lint contracts/`, `cargo tree -p aprender-mcp-laya-train-lambda -e normal`, `pmat query`, tool version probes

### Secondary (MEDIUM confidence)
- Owner-run command shapes inferred from recipe docstrings and the 14-22 runbook (not executed)

### Tertiary (LOW confidence)
- Causal explanation of the CUDA-vs-CPU gap and the probability of landing in band (flagged `[ASSUMED]`)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH, no new packages; versions read from binaries
- Architecture (consumer map, record compatibility constraint, image/finalizer ripples): HIGH, verified in code and by `cargo tree`
- Candidate table: HIGH on arithmetic (recomputed), MEDIUM on interpretation
- Owner-run chain: MEDIUM, derived from committed recipes
- Pitfalls on D-14 levers: HIGH (gate contract and `check_recipe_block` read)
- D-16 outcome: LOW (single draw)

**Research date:** 2026-10-31
**Valid until:** 2026-11-28 (the motivation-v1-sm S3 objects expire about then; also re-read the tree facts if any commit touches `contracts/` or `crates/aprender-decide` first)
