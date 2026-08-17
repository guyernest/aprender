# Phase 5 Plan 01 — Production Calibration Measurements

**Purpose:** the measured evidence input to the D-04 human checkpoint (plan 05-03's deliberate
three-place contract edit). This file MEASURES. It edits no contract, no threshold table and no
gate.

**Host:** local dev box (Darwin 25.6.0, arm64), CPU only — not lambda-vector.
**Branch:** `gsd/phase-2-contract-gate` (02-01 policy; no PR).
**Harness:** `production_calibration_matrix` in
`crates/aprender-train/src/train/setfit/evidence.rs`, `#[ignore]`d and env-gated on
`APRENDER_MINILM_DIR`.

**Gate-independence (why measuring is legal today):** the `UncalibratedRegime` refusal lives in
`validate_evidence` (`tune.rs`, judgement time). `run_tuning` and
`UpdateEvidence::from_tune_output` carry no regime gate, so the production envelope can be
measured on today's code with zero relaxation. Nothing in this plan widens a gate.

---

## Frozen production hyperparameters

Read from the **pinned, hash-locked `uv` environment**, not from documentation, memory, or the
web (`scripts/setfit_fixtures/`, `setfit==1.1.3`).

### The literal command

```bash
cd scripts/setfit_fixtures && uv run python -c \
  "from setfit import TrainingArguments; a = TrainingArguments(); \
   print(a.num_epochs, a.batch_size, a.body_learning_rate)"
```

### Its stdout

```text
(1, 16) (16, 2) (2e-05, 1e-05)
```

### The environment those numbers came from

```text
setfit 1.1.3
transformers 4.57.6
sentence_transformers 5.7.0
torch 2.13.0
num_epochs (1, 16)
batch_size (16, 2)
body_learning_rate (2e-05, 1e-05)
max_length None
warmup_proportion 0.1
```

### Reading the tuples

Each of the three is a `(body, head)` pair. `SetFitTrainConfig` configures the **contrastive
body** stage — the only stage the evidence table measures — so the **body** member is the one
that maps onto its knobs. The head members (`16` epochs, batch `2`, lr `1e-05`) belong to the
logistic head, which this gate does not measure.

### The frozen values

```text
epochs = 1
batch  = 16
```

(reference encoder lr = `2e-05`, the harness's REAL condition.)

**Cross-check against the in-repo recipe.** `crates/aprender-train/src/train/setfit/config.rs`
already carries `REFERENCE_EPOCHS = 1`, `REFERENCE_BATCH_SIZE = 16`,
`REFERENCE_ENCODER_LR = 2e-5`. The harness **asserts** this agreement at construction
(`production_config`), so a future divergence between the pinned Python env and the Rust
reference recipe turns the harness red instead of silently measuring cells that production runs
never enter.

**Consequence (F-R3):** every cell label this milestone can enumerate is
`s{shots}e1b16`, and therefore the D-02 contract entry's `cells=` component is
`s8e1b16,s16e1b16,s32e1b16,s64e1b16`. These two values were frozen **before** any calibration
pass, which is the ordering the plan's must-have truth requires — they are baked into every cell
label and cannot be chosen after seeing a result.

---

## Probe timing and projection

The probe ran **before** the boundary matrix, and the matrix projection below is derived from it.
That ordering is the point (CLAUDE.md: check in BEFORE >1 hr compute on non-lambda-vector hosts).

### The literal command

```bash
CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 \
  cargo test -p aprender-train --lib --features setfit production_calibration \
  -- --ignored --nocapture > /tmp/probe.log 2>&1
rc=$?
```

Status is read on the next line, never through a pipe (CLAUDE.md verification rule 1). The
timing wrapper was a throwaway script outside the tracked tree; it is exactly:

```bash
START=$(date +%s); START_ISO=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 \
  cargo test -p aprender-train --lib --features setfit production_calibration \
  -- --ignored --nocapture >/tmp/probe.log 2>&1
rc=$?
END=$(date +%s)
echo "probe_wall_clock_secs=$((END - START)) rc=${rc}"
```

### Measured (debug profile)

```text
probe_start_iso=2026-08-17T04:11:31Z
probe_end_iso=2026-08-17T04:41:42Z
probe_wall_clock_secs=1811
rc=0
```

Breakdown, from the harness's own printed report and the libtest summary:

| Quantity | Value |
|---|---|
| whole `cargo test` invocation | 1811 s (30.2 min) |
| — of which incremental recompile + binary start | ~46 s |
| libtest `finished in` | 1764.62 s |
| `run_tuning` alone (harness `wall_clock`) | 1758.6 s |
| fixed per-pass overhead (corpus + checkout load + prepare + evidence) | ~6.0 s |
| optimizer steps in the pass | 24 |
| **cost per optimizer step** | **73.3 s/step** |

### The regime id the run rendered

Quoted verbatim from `/tmp/probe.log`:

```text
CALIBRATION REGIME: minilm-slice-h384-l6-a12-i1536-v30522@1110a243
```

```text
minilm-slice-h384-l6-a12-i1536-v30522@1110a243|seeds=13|cells=s8e1b16
```

It begins with the required `minilm-slice-h384-l6-a12-i1536-v30522@1110a243` — the
`minilm-slice-` prefix is hardcoded in `BertSentenceEncoder::architecture_fingerprint` and
renders for the full model too (F-R2), and the `h384-l6-a12-i1536-v30522` dimensions prove the
**production** encoder was loaded, not the 97-token slice. This string was **rendered by the
production code path** (`calibration_regime_id`), never composed by the harness (T-05-01-01).

### Projection arithmetic for the 18-pass boundary matrix

Step count per cell is set by the **contracted default pair budget**, not by the shot count
directly. For a uniform selection of `n` rows across `K = 3` classes
(`aprender-contrastive-data/src/pairs.rs`):

```text
positive_capacity = K · C(n,2)      = 3 · n(n−1)/2
negative_capacity = C(K,2) · n²     = 3 · n²
budget            = 2 · max(pos, neg) = 6n²      (negatives dominate for all n ≥ 1)
steps             = ceil(budget / batch) = ceil(6n² / 16)
```

| cell | pos_cap | neg_cap | budget | steps @ b16, e1 | steps relative to s8 |
|---|---|---|---|---|---|
| `s8e1b16`  | 84   | 192   | 384    | **24**   | 1× |
| `s16e1b16` | 360  | 768   | 1536   | 96       | 4× |
| `s32e1b16` | 1488 | 3072  | 6144   | 384      | 16× |
| `s64e1b16` | 6048 | 12288 | 24576  | **1536** | **64×** |

The measured `steps=24` for the probe confirms the closed form against the running code.

> **The plan's `~8×` weighting is REFUTED by this measurement.** 05-01 projected "s64 cells
> weighted ~8x s8 (rows scale)". Rows do scale 8×, but the *budget* is **quadratic** in shots
> because `negative_capacity = 3n²` dominates, so an s64 pass is **64×** an s8 pass, not 8×.
> Correcting this is the difference between a ~6-hour projection and a ~12-day one, so it is
> recorded here rather than silently applied.

**Debug-profile projection**, at 73.3 s/step + 6.0 s fixed per pass:

| group | passes | steps/pass | s/pass | subtotal |
|---|---|---|---|---|
| `s8e1b16` × {13,31,53} × {real, ctrl, near-null} | 9 | 24 | 1 765 s | 15 881 s (4.4 h) |
| `s64e1b16` × {13,31,53} × {real, ctrl, near-null} | 9 | 1 536 | 112 556 s | 1 013 004 s (281.4 h) |
| **total** | **18** | — | — | **1 028 885 s ≈ 285.8 h ≈ 11.9 days** |

### Release-profile cross-check (and a result that matters for 05-03)

285 hours is far enough over the compute gate that "a release build would fix it" had to be
**measured**, not assumed (CLAUDE.md verification rule 2). It is also the more faithful profile:
a user's `apr setfit train` is a release binary (`cargo install aprender`).

```bash
CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 \
  cargo test --release -p aprender-train --lib --features setfit production_calibration \
  -- --ignored --nocapture > /tmp/probe_release.log 2>&1
```

```text
release_probe_start_iso=2026-08-17T04:42:24Z
release_probe_end_iso=2026-08-17T04:46:04Z
release_probe_wall_clock_secs=220
rc=0
```

| Quantity | debug | release | ratio |
|---|---|---|---|
| `run_tuning` wall clock (s8, 24 steps) | 1758.6 s | **51.0 s** | 34.5× |
| per optimizer step | 73.3 s | **2.125 s** | 34.5× |
| fixed per-pass overhead | ~6.0 s | ~0.2 s | — |

**The two profiles produced BIT-IDENTICAL measurements.** Every printed relative delta, every
binding parameter, every `delta_norm` / `init_norm` / `grad_norm_max` / noise floor agrees to
the last printed digit across the two runs — e.g. `embedding real_min 1.813e-3`,
`projection_weight real_min 1.231e-3`, `attention_key_bias real_min 1.714e-7`, and the binding
row `embeddings.word_embeddings.weight … delta_norm=1.159e-2 init_norm=1.904e2`.

This is a load-bearing result for 05-03, not a performance footnote: **the profile is not a
degree of freedom in the calibration.** Whatever ε is frozen from a release-profile matrix is
the same ε a debug-profile matrix would have produced, so choosing release for the compute
budget does not change what is being measured.

**Release-profile projection**, at 2.125 s/step + 0.2 s fixed per pass:

| group | passes | steps/pass | s/pass | subtotal |
|---|---|---|---|---|
| `s8e1b16` × {13,31,53} × {real, ctrl, near-null} | 9 | 24 | 51.2 s | 461 s (7.7 min) |
| `s64e1b16` × {13,31,53} × {real, ctrl, near-null} | 9 | 1 536 | 3 264 s | 29 378 s (8.16 h) |
| **total** | **18** | — | — | **29 839 s ≈ 8.29 h** |

### Preliminary observation carried into Task 2 (not a conclusion)

`attention_key_bias` — the class the fixture leaves **ungated** on the gradient-free argument
(`dL/db_k = 0` by softmax shift-invariance) — is not bit-frozen at production scale:
`grad_norm_max = 8.084e-10`, `real_min relative_delta = 1.714e-7`, about 150× its own
`rounding_noise_floor` of `1.133e-9`. Whether that is f32 reduction residue or a real gradient
cannot be decided without the 1e-30 and 1e-8 controls, which is precisely what the boundary
matrix runs. Recorded here so the Task 2 re-derivation is answering a question that was already
open, rather than one invented after seeing its own result.

---

## COMPUTE GATE — checkpoint reached, boundary matrix NOT run

CLAUDE.md, "Check in BEFORE acting": *compute spend > 1 hr on non-lambda-vector hosts*
(lambda-vector is pre-authorized). Plan 05-01 Task 1 step (4) restates it as a hard 60-minute
projection check before the matrix.

**Projection: 8.29 h (release) / 285.8 h (debug). Both exceed 60 minutes by more than 8×.**
Execution stopped here. The boundary matrix has NOT been run, Tasks 2 and 3 are not started, and
no ε has been derived.

### Why this is larger than the plan expected

Two independent factors, both measured above:

1. **The pair budget is quadratic in shots, not linear.** The plan projected s64 at ~8× s8
   ("rows scale"). The contracted default budget is `2·max(pos_cap, neg_cap)` and
   `neg_cap = 3n²` dominates, so s64 is **64×** s8. This alone is an 8× projection error.
2. Nothing about the per-step cost was surprising — 2.1 s/step in release for a 22M-parameter
   encoder over 32 texts is ordinary. The matrix is expensive because it is 18 passes of which
   nine are 1 536-step passes.

### Options for the human (go / no-go)

| # | Option | Measured/derived cost | What it costs in evidence |
|---|---|---|---|
| A | Run the full matrix locally, **release** profile | **8.29 h** wall clock on this dev box | Nothing — this is the plan as written, at the measured price. Release is bit-identical to debug (proven above). |
| B | Move the matrix to **lambda-vector** (pre-authorized) | 8.29 h scaled by that host's CPU; not measured here | Nothing, plus it removes the check-in requirement entirely. Needs the 86.7 MB checkout materialized there. |
| C1 | Trim the shot boundary to **{s8, s16}** | 9×51.2 + 9×204.2 = **2 299 s ≈ 38 min** — under the gate | Weakest. s32/s64 would be covered by extrapolating a 16×/64× budget factor the matrix never observed. |
| C2 | Trim the shot boundary to **{s8, s32}** | 9×51.2 + 9×816.2 = **7 807 s ≈ 2.17 h** | Moderate: s64 covered by a single 4× budget extrapolation. Still over the gate. |
| C3 | Keep {s8, s64}, trim to **one seed (31, the median)** | 3×51.2 + 3×3 264 = **9 947 s ≈ 2.76 h** | Loses the cross-seed spread, which is what makes the window a window rather than one run's number. Still over the gate. |
| D | Keep all cells but **pin an explicit small pair budget** | ~38 min | **Not recommended, and flagged rather than offered as equal.** The cell label `s{shots}e{E}b{B}` does not record the budget, so an entry claiming `s64e1b16` coverage measured at a trimmed budget would label cells that trained 64× less than the runs the entry licenses. That is a fidelity break, not a trim. |

**Recommendation (mine, non-binding): B, else A.** The measurement is the keystone of the whole
milestone (D-01), the ε it produces gates every Phase 5 benchmark cell, and options C1–C3 all
buy time by making the margin argument weaker in exactly the dimension the cross-AI review
already flagged as the plan's soft spot (six measured cells as an engineering margin for the
other 34). Paying 8.3 h once is cheaper than defending a thinner envelope at the D-04 checkpoint.

### What is already in hand regardless of the decision

- The harness (`production_calibration_matrix`) exists, is `#[ignore]`d, env-gated, and green.
- E/B are frozen from the pinned env with the command and stdout recorded.
- The production regime id is rendered and quoted verbatim.
- Debug/release measurement equivalence is proven.
- The plan's 8× projection error is corrected to 64×.

Resuming after a go/no-go needs only Task 2 onward; nothing above is re-run.

---

## COMPUTE GATE RESOLVED — Option B chosen; dispatch BLOCKED

**Human decision (recorded 2026-08-17): Option B — run the full contracted boundary matrix on
the lambda-vector host.** Full matrix as specified: `{s8, s64}` × seeds `{13, 31, 53}` ×
`{real, control, near-null}` = 18 passes. Trims C1/C2/C3 rejected; option D rejected. The
rationale on record: lambda-vector is pre-authorized for compute per CLAUDE.md, so the >1 hr
check-in requirement is *removed rather than waived*, and it costs zero evidence.

**The dispatch could not be performed: `lambda-vector` is not reachable from this host.**

### The reachability evidence (mechanism, not intent — CLAUDE.md verification rule 2)

Executing host, read from the machine rather than assumed:

```text
$ hostname
MacBook-Pro-7.local
$ uname -a
Darwin MacBook-Pro-7.local 25.6.0 ... RELEASE_ARM64_T6041 arm64
```

Every resolution and connection path was tried, and each failed:

| Probe | Command | Result |
|---|---|---|
| DNS / mDNS | `ping -c 1 lambda-vector` | `ping: cannot resolve lambda-vector: Unknown host` |
| Directory service | `dscacheutil -q host -a name lambda-vector` | no records returned |
| SSH (non-interactive) | `ssh -o BatchMode=yes -o ConnectTimeout=5 lambda-vector hostname` | **rc=255** — `ssh: Could not resolve hostname lambda-vector: nodename nor servname provided, or not known` |
| SSH client config | `grep -i '^Host ' ~/.ssh/config` | one entry only: `rvsc  ec2-54-72-80-139.eu-west-1.compute.amazonaws.com` — an AWS EC2 box, not lambda-vector |
| Known hosts | `grep -ci lambda ~/.ssh/known_hosts` | `0` |
| Overlay network | `command -v tailscale` | not installed |
| In-repo dispatch path | `grep -rIn ssh scripts/` | no script dispatches to lambda-vector; the three `scripts/` hits naming it are prose comments about its disk layout and GPU, not a connection |
| Named pre-auth doc | `~/.claude/projects/…/memory/feedback_compute_pre_authorized.md` | not present (the memory dir holds six unrelated files) |

Every status above was read directly (`cmd > log 2>&1; rc=$?`), never through a pipe
(CLAUDE.md verification rule 1).

**The matrix was NOT run locally.** Falling back to the local box would have spent the 8.29 h the
human's decision explicitly redirected, and would have converted a pre-authorized spend into an
unauthorized one. Execution halted instead.

### A caveat the next dispatcher needs

lambda-vector is a **GPU** host, but this measurement is **CPU-bound by construction**:
`production_config` sets `device: "cpu"`, and the SetFit contrastive trainer has no GPU path.
So option B's benefit is *authorization*, not speed — exactly as the human's rationale stated.
The 8.29 h figure is this box's Apple-silicon CPU; **lambda-vector's wall-clock is unknown and
could be higher or lower**, and must be re-projected there with the same one-cell probe before
the full matrix is launched. Do not carry 8.29 h over as if it were a measurement of that host —
that is precisely the "label a run by intent" error.

### Exact commands to run once a dispatch path exists

```bash
# 0. prerequisite: materialize the 86.7 MB pinned checkout on the target host
cd scripts/setfit_fixtures && uv run python fetch_full_weights.py

# 1. re-project on THAT host first (one cell, REAL only) — same gate discipline
CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 \
  cargo test --release -p aprender-train --lib --features setfit production_calibration \
  -- --ignored --nocapture > /tmp/probe.log 2>&1
rc=$?

# 2. the full 18-pass boundary matrix (probe env var unset)
CARGO_INCREMENTAL=0 \
  cargo test --release -p aprender-train --lib --features setfit production_calibration \
  -- --ignored --nocapture > /tmp/matrix.log 2>&1
rc=$?
```

`--release` is correct and is **not** a weakening of the calibration: the debug/release
bit-identity proven above means the profile is a price, not a degree of freedom. A reader who
does not know that will assume a release-mode calibration is the weaker one, so it is stated
here explicitly as well as in the cross-check section.

The harness writes its report to `$SETFIT_PRODUCTION_CALIBRATION_REPORT` (default
`$TMPDIR/setfit-production-calibration.txt`), which is the artifact to transport back — a
summarizing wrapper around `cargo test` drops `--nocapture` output entirely.

### What Task 2 still owes when it runs

- 6 per-cell tables (`s8`/`s64` × seeds 13/31/53) over every `ParameterClass`, with the columns
  the contract's DERIVATION invariant names.
- `ctrl_max < real_min` recorded for every gated class in every cell (the harness asserts it).
- At least one full regime id quoted verbatim, architecture component
  `minilm-slice-h384-l6-a12-i1536-v30522@1110a243`.
- The **`attention_key_bias` verdict**, which is the open question from the probe and is
  load-bearing for 05-03: at production scale that class shows
  `grad_norm_max = 8.084e-10` and `real_min relative_delta = 1.714e-7`, about 150× its own
  `rounding_noise_floor` of `1.133e-9`. The fixture leaves the class **ungated** on the
  gradient-free argument (`dL/db_k = 0` by softmax shift-invariance). The 1e-30 and 1e-8
  controls decide whether that argument survives production scale. If it does **not**, that
  changes 05-03's proposed regime entry and must be surfaced loudly, not absorbed.

