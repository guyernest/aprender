# 05-12 Task 1 — the wall-clock projection, written down BEFORE any cell ran

Recorded 2026-09-08, at `HEAD = 00350eb60`, branch `gsd/phase-2-contract-gate`.

This file exists so the projection can be **compared against the outcome afterwards**. An
unrecorded projection cannot be; a wall-clock surprise then becomes a feeling rather than a
finding. Nothing in this plan had run any cell when this file was written — `benchmarks/` did
not exist on disk (`ls benchmarks/` → `No such file or directory`).

---

## 1. The precondition, checked read-only before anything else

05-11's retarget must be committed and the expectation set must be 40, or the first cell would
write a run manifest declaring cells that can never exist.

| Check | Command | Result |
|---|---|---|
| contract scope | `grep -n 'expected_cells' contracts/setfit-benchmark-claims-v1.yaml` | `expected_cells: 40` (line 165); `deferred_expected_cells: 80` nested under `deferred_two_method_scope` (line 221) |
| code scope | `grep -n 'ACTIVE_METHODS' crates/aprender-train/src/train/setfit/bench_row.rs` | `pub const ACTIVE_METHODS: [Method; 1] = [Method::Setfit];` (line 91); `EXPECTED_CELLS` derives from it |
| retarget committed | `git log --oneline -6` | `78a8af6c5`, `bc4c9a65b`, `4a50b88a3`, `d5b6759bc`, `f54f5f1b3`, `00350eb60` all present |
| suite green at 40 | 05-11 SUMMARY | `make setfit-bench-tests` rc=0 at `f54f5f1b3`; HEAD is one **docs-only** commit later |

**Precondition MET.**

## 2. The binary, pinned

```
$ . scripts/apr_bin.sh
rc=0
APR=/Users/guy/Development/machine-learning/aprender/target/release/apr
$ "$APR" --version
apr 0.63.0 (00350eb60)
```

The embedded SHA equals HEAD, so the projection and the sweep describe the same build. **No
release build is needed**, so no build time enters the projection.

## 3. The cost law — derived, not assumed

SetFit's contrastive pair budget is **quadratic in shots**, which 05-01 measured and recorded
after refuting its own plan's linear `~8x` guess:

```
budget = 6n^2          steps = ceil(budget / batch) = ceil(6n^2 / 16)     (b16, e1)
```

| cell | steps | relative to s8 |
|---|---|---|
| `s8`  | 24 | 1x |
| `s16` | 96 | 4x |
| `s32` | 384 | 16x |
| `s64` | **1536** | **64x** |

Per seed: **2 040 steps**. Over the 40-cell matrix (4 shot levels x 10 seeds): **20 400 steps**.

`steps=24` was confirmed against the running code by a measured probe, so this is the closed
form the binary actually executes, not a formula from a document.

## 4. The projection — TWO measured bases that DISAGREE, both stated

There are two measured bases on this host, and they do not agree at s8. Reporting only the
convenient one would be the defect CLAUDE.md rule 1 exists to prevent, so both are given.

### Basis A — 05-01's calibration-tier per-step cost (conservative, the number to authorize against)

Measured, release profile, this box: **2.033 s/step at s8** (439.4 s tuning / 9 passes / 24
steps) and **2.113 s/step at s64** (a single measured pass, `wall_clock=3246.2s`, `steps=1536`
— 4% higher because a larger selection touches more embedding rows). Interpolated linearly in
`log2(shots)` between the two measured ends.

| tier | steps/cell | s/step | cell tuning | x10 seeds |
|---|---|---|---|---|
| s8  | 24 | 2.033 | 48.8 s | 488 s (0.14 h) |
| s16 | 96 | 2.060 | 197.7 s | 1 977 s (0.55 h) |
| s32 | 384 | 2.086 | 801.2 s | 8 012 s (2.23 h) |
| s64 | 1536 | 2.113 | 3 245.6 s | **32 456 s (9.02 h)** |
| **tuning total** | | | | **42 932 s = 11.93 h** |

Plus the per-cell non-tuning work, which a projection built from tuning alone would leave out:
the encoder load, the 87 MB APR write, the reload through the production door, **the dedicated
cold-probe child** (a fresh process + artifact load + one classify), 3 warm classifies,
throughput over the 280-row test split in 9 batches of 32, and the validation (66 rows) + test
(280 rows) evaluations. Budgeted at **60 s/cell** — an upper estimate, since 05-07 measured a
whole s8 CLI chain (train + inspect + two evals over the same 66 and 280 rows + predict) at
~37 s.

```
BASIS A TOTAL = 42 932 s + 40 x 60 s = 45 332 s = 12.59 h
```

**The s64 tier alone is 75.6% of it.**

### Basis B — 05-07's CLI-tier measured whole-chain s8 cell

05-07 measured the production s8/seed-13 chain end to end at **~37 s wall, release**. That is
*less* than Basis A's s8 tuning alone (48.8 s), so the two tiers genuinely disagree and the
disagreement is not resolved here by picking one.

| assumption about the 37 s | implied s/step | matrix total |
|---|---|---|
| all 37 s is training (upper edge of Basis B) | 1.542 | **8.74 h** |
| 60% is training, 25 s/cell overhead | 0.925 | **5.52 h** |

### The number

> **Projection: 12.6 h (Basis A, conservative). Plausible floor ~5.5 h. The honest range is
> 5.5 h – 12.6 h.**

The weakest input is the per-step cost at the CLI tier, which is exactly what the pilot cell
measures — see the recommendation below.

## 5. Disk — the recurring failure mode on this host

| figure | value |
|---|---|
| free now | **36.4 GiB** (96% capacity, `df -k .`) |
| projected peak write | 40 artifacts x 87 MB = **3.40 GiB**, plus the attested dataset (~MB) and per-cell logs |
| projected free after | **~32.9 GiB** |

`CARGO_INCREMENTAL=0` is exported by `scripts/run_bench_cells.sh` (line 68). That is a
**mitigation, not a guarantee** — `target/debug/incremental` filled this volume three times in
this milestone, and 05-01's `s64:13` chunk died to a full volume. No cargo build is expected
during the sweep (the binary is already fresh), which removes the main way the cache grows.

`benchmarks/tweeteval-stance/artifacts/` is **not** covered by `.gitignore` (line 50 is
`/*.apr`, root-anchored per CB-510). 3.4 GiB of `.apr` must never be staged; only rows,
selections, locks and the run manifest are committed.

## 6. What is NOT on the table

- **Parallelism.** Concurrent cell processes would make every `train_peak_rss_bytes`,
  `cold_latency_ms`, `warm_latency_ms_median` and `throughput_rows_per_sec` a measurement of a
  contended machine — not the quantity EVAL-05 contracts — and would race the shared run
  manifest. `scripts/run_bench_cells.sh` has no dispatch flag by design and holds a single-writer
  pid lock. An overrun comes back as a checkpoint.
- **Thinning the protocol.** Not fewer warmups, not a skipped cold-probe child, not a sampled
  peak where a child-measured one is contracted, not a reduced seed or shot set.
- **A second host.** D-19 established there is none: `ssh lambda-vector` → rc=255,
  `Could not resolve hostname`. The one AWS instance that exists is a t3.small.

## 7. The measurement asymmetry this authorization also accepts

On this platform the **TRAIN** peak RSS is a `sysinfo`-sampled **lower bound** carrying its
sample-interval field, while the **INFERENCE** peak is a true kernel high-water mark from
`/usr/bin/time -l` in a dedicated fresh child. The two are never averaged into one figure; the
asymmetry is declared per row and tallied over all 40 rows, and 05-13 renders them as two
separately labelled figures.

## 8. Recommendation

**option-b.** Task 2's pilot cell exists regardless — it is the gate proof — so authorizing the
pilot first costs nothing extra and replaces the projection's weakest input (an extrapolated
per-step cost, where the two measured bases differ by ~2.2x) with one measured cell of exactly
the shape being run. The remaining 39 are then re-presented against a measured number rather
than a 5.5–12.6 h range.
