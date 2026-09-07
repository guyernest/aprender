---
phase: 06
phase_name: native-time-series-forecasting-stack
reviewed: 2026-09-06T00:00:00Z
depth: standard
scope: incremental
diff_base: ce3e5a8ea1ecbb3d873ce81972e099f8b4391758
reviewed_files: 9
files_reviewed_list:
  - crates/aprender-forecast/src/forecast.rs
  - crates/aprender-forecast/src/prophet.rs
  - crates/aprender-forecast/src/types.rs
  - crates/aprender-mcp-forecast/src/lib.rs
  - contracts/forecast-tool-boundary-v1.yaml
  - contracts/prophet-parity-v1.yaml
  - contracts/aprender/binding.yaml
  - justfile
  - .planning/phases/06-native-time-series-forecasting-stack/COVERAGE.md
findings:
  critical: 1
  warning: 4
  info: 4
  total: 9
critical: 1
warning: 4
info: 4
status: issues-found
---

# Phase 06 — Incremental Code Review (gap-closure plans 06-10 … 06-13)

**Reviewed:** 2026-09-06
**Depth:** standard (per-file, with targeted A/B measurement)
**Diff:** `ce3e5a8ea1ecbb3d873ce81972e099f8b4391758..HEAD`, +1016 / −103 across 9 files
**Status:** issues-found — **1 Critical, 4 Warning, 4 Info**

## Summary

Three of the four claimed gap closures hold up under adversarial inspection. The two
items the brief flagged as most likely to be defective are **not** defective, and I
checked them rather than accepting the summaries:

- **Integer overflow in the new cost arithmetic — not reachable.** `design_cells =
  (ds.len() + args.horizon) * holiday_columns` (`forecast.rs:258`) is guarded by three
  refusals that all execute earlier in the same function: `ds.len() > MAX_POINTS`
  (line 48), `horizon > MAX_HORIZON` (line 54) and the in-loop
  `holiday_columns > MAX_HOLIDAY_COLUMNS` (line 216). Maximum product 23 650 000. The
  accumulator `holiday_dates_total` is likewise bounded: the loop cannot run more than
  1001 iterations (each iteration adds ≥ 1 to `holiday_columns`, which is refused at
  1001) and each term is ≤ 1000, so the sum is ≤ 1.001e6. No wrap, no bypass.
- **The `HashSet` rewrite is an exact membership rewrite.** `d + off == day` ⟺
  `d == day - off` holds in ℤ, and both sides are bounded far from i64 overflow:
  `parse_date` (`dates.rs:78-112`) admits only 4-digit years, so `day ∈ [-719528, 2932896]`,
  and `off` is bounded to ±365 by the window check at `forecast.rs:191`. Negative offsets
  are handled correctly by the subtraction. The sets are built once per call —
  `make_design` at `prophet.rs:272` and `predict` at `prophet.rs:766` — outside the row
  loop, and `feature_row` has no other caller in the workspace.
- **The cap refusal does not over-refuse and the three messages stay distinct.** The
  refusal at `forecast.rs:159` runs after the `growth` enum is parsed (so an unknown
  growth string still wins), before the logistic branch (so `logistic growth needs cap`
  and `cap C must exceed max(y)` keep their own text), and before `make_design`
  (line 289). The positive control in
  `forecast::tests::an_option_belonging_to_the_other_model_is_refused_not_dropped`
  proves `growth: "logistic"` with a valid cap still fits. I ran the suite: 16 passed,
  0 failed.
- **`justfile` discipline is correct.** `rc=$?` at line 811 is read from a redirect, not
  a pipe; a missing `HOLIDAY DESIGN WALL:` line fails hard; the `total_s` parse is by
  token, not column; the `profile=release` `case` guard is real.
- **No `unwrap()` introduced**, and no `expect()` on request-derived input in the door or
  server path. `MAX_HOLIDAY_DESIGN_COST`, `MAX_HOLIDAY_DATES_TOTAL` and `DEFAULT_POOL`
  are each genuinely covered by `types::tests::cost_bounds_match_contract`.

What is wrong is the fourth item. **06-13's Poisson fix removes an accidental cap on an
unbounded, attacker-controlled quantity and adds no door bound to replace it.** I
measured it A/B against the pre-fix binary: an accepted 1 132-byte request now costs
**14.8× more wall** and crosses the same 2 s SC1 bar that 06-11/06-12 were created to
defend. Every bound this phase added is on the *holiday* axis; the logistic-uncertainty
axis is unbounded, and the harness 06-13 added to watch it can only see the one `freq`
value that does not exhibit the problem.

---

## Critical Issues

### CR-01: 06-13 unbounded the simulated changepoint count; a 1.1 KB accepted request now walls at 2.33 s (14.8× regression, over the SC1 bar)

**File:** `crates/aprender-forecast/src/prophet.rs:713-729` (`poisson`), reached from
`prophet.rs:875-876`; no corresponding bound anywhere in
`crates/aprender-forecast/src/forecast.rs`.

**Issue.**
`predict`'s logistic arm draws `lambda = d.changepoints_t.len() * (t_max - 1.0)`
(`prophet.rs:874-876`) once per uncertainty sample (`uncertainty_samples = 1000`,
`prophet.rs:71`) and then builds, sorts and re-evaluates `n_changes` synthetic
changepoints per sample. `t_max` is `(last_future_day - start_days) / t_scale_days`
(`prophet.rs:759-761`), where `t_scale_days` is the **history span** and the future span
is set by `horizon × freq step`. Nothing at the door bounds that ratio:
`MAX_SPAN_DAYS` bounds the history span, `MAX_HORIZON` bounds the *count* of future
steps, and `future_days` (`dates.rs:120-139`) multiplies that count by 7 for `"W"` and
by ~30.4 for `"MS"`. For 33 daily points (span 32 d) with `horizon: 3650, freq: "MS"`,
λ ≈ 25 × 3469 ≈ **86 700** simulated changepoints per sample, 1000 samples.

Before 06-13 this was harmless *by accident*: Knuth's product method saturated near 745
for any λ > 745.13, which acted as an unintended hard cap. 06-13 correctly identified
that as a numerical defect and fixed it — but the fix makes the count track λ, and λ is
unbounded from the door. `FIT_BUDGET_SECS` cannot help: it is entered inside
`fit_prophet`, and this cost is in `predict`, which `forecast.rs:290-293` calls with no
budget at all.

**Concrete trigger (measured, not argued).** Release build, `--http --pool 1`, min of 2
runs each, same host, same requests, pre-fix binary built from the review base commit
`ce3e5a8ea`:

| request (all accepted, all inside every door bound) | pre-06-13 | HEAD | factor |
|---|---|---|---|
| 33 pts / 1 d spacing, `horizon: 3650`, `freq: "MS"`, `growth: "logistic"`, `cap: 50` | 0.157 s | **2.334 s** | **14.8×** |
| same but 30 d point spacing (λ ÷ 30) — *control* | 0.152 s | 0.209 s | 1.38× |
| same but `freq: "W"` | 0.164 s | 0.634 s | 3.87× |
| same but `freq: "D"` | 0.173 s | 0.231 s | 1.34× |

Request payload: **1 132 bytes**. Reproduced three times at 2.375 / 2.351 / 2.316 s.
The mechanism is pinned by the control pair, not asserted: the two logistic rows differ
*only* in history spacing — identical point count, identical horizon, identical row
count — so the 11× spread between them is attributable to λ and nothing else, and the
matching `growth: "linear"` pair is flat (0.147 s / 0.149 s), which is the arm that does
not call `poisson`.

2.334 s is over SC1's 2 s bar, from an unauthenticated request smaller than this
paragraph, against a default pool of K = 8.

**Fix.** Bound λ at the door, the same way 06-11/06-12 bounded the holiday product, and
own the constant in `contracts/forecast-tool-boundary-v1.yaml` so
`types::tests::cost_bounds_match_contract` mirrors it:

```rust
// forecast.rs, in the prophet arm, BEFORE make_design — the future span is already
// known from `fut`, and n_changepoints is bounded by Spec::default_linear.
if growth == Growth::Logistic {
    let t_scale = (ds[ds.len() - 1] - ds[0]) as f64;      // > 0: ds is strictly ascending
    let t_max = (fut[fut.len() - 1] - ds[0]) as f64 / t_scale;
    let lambda = f64::from(u32::try_from(spec_n_changepoints)?) * (t_max - 1.0);
    if lambda > MAX_LOGISTIC_CHANGEPOINT_LAMBDA {
        return Err(ForecastError::Validation(format!(
            "logistic uncertainty draws {lambda:.0} simulated changepoints per sample \
             (n_changepoints x (t_max - 1)), which exceeds \
             max_logistic_changepoint_lambda {MAX_LOGISTIC_CHANGEPOINT_LAMBDA}; \
             shorten the horizon, use freq D, or send a longer history"
        )));
    }
}
```

A clamp inside `poisson` (`n_changes.min(CAP)`) would also remove the wall but silently
reintroduces the truncation 06-13 exists to remove — refuse at the door instead, which
is this phase's own stated discipline (D-11). Either way, add `freq` to the
`logistic_band_wall` harness and give it a `just` recipe with a bar (see WR-04).

---

## Warnings

### WR-01: `POISSON_NORMAL_BRANCH_LAMBDA` is the one new behavioural constant with no contract mirror — and the new test cannot detect a change to it

**File:** `crates/aprender-forecast/src/prophet.rs:686`

**Issue.** Every other bound this phase added is contract-owned and asserted equal to the
YAML (`types.rs:249-266`). The branch threshold that decides whether a request gets an
*exact* Poisson or a *normal approximation* is a bare Rust literal. The contract mentions
"the 30.0 branch threshold" only in prose
(`contracts/prophet-parity-v1.yaml`, `poisson_sampler_domain` invariants), so the value is
written twice with nothing asserting the two agree — exactly the D-15 failure mode the
rest of the diff is careful about.

**Concrete trigger (measured).** I re-ran the sweep's arithmetic with the threshold
effectively lowered below 5.0, so every lambda in `LAMBDAS`
(`prophet.rs:966`) takes the normal branch, using the same `Rng`, the same
N = 20 000 and the same seed:

```
lambda=     5.0 mean=    4.9985 rel=0.000310
lambda=    29.0 mean=   29.0210 rel=0.000726
lambda=    31.0 mean=   31.0664 rel=0.002144
lambda=   100.0 mean=  100.0811 rel=0.000811
lambda=   900.0 mean=  900.0384 rel=0.000043
lambda=  2839.0 mean= 2839.5755 rel=0.000203
```

Every point is under the 0.01 bar. And
`wp_log_r_logistic_fixture_lambda_is_far_below_the_branch_threshold`
(`prophet.rs:1005-1038`) only asserts `3.1239 < THRESHOLD`, which passes for any
threshold above 3.13. So lowering the constant to 3.2 — replacing the exact sampler with
a normal approximation for *every* real logistic request, in the λ < 30 regime where the
contract itself says the approximation is not standard — leaves the entire suite green.
The guard catches loosening in the "make it Knuth again" direction only.

**Fix.** Add `poisson_normal_branch_lambda: 30` to
`contracts/forecast-tool-boundary-v1.yaml` (or a numeric key on the
`poisson_sampler_domain` equation) and assert it in
`types::tests::cost_bounds_match_contract` alongside the others; add one sweep point
*below* the threshold whose bar is tight enough that the normal branch fails it (e.g. a
χ² or variance check at λ = 5).

### WR-02: the new falsification test asserts only the first moment; a zero-variance stub passes it

**File:** `crates/aprender-forecast/src/prophet.rs:969-995`
(`poisson_mean_tracks_lambda_across_its_whole_domain`);
`contracts/prophet-parity-v1.yaml`, `poisson_sampler_domain`.

**Issue.** The contract's `codomain` states "a non-negative count whose mean **and
variance** are both lambda", and `FALSIFY-PROPHET-013`'s `if_fails` describes the failure
in variance terms ("the simulated trends spread less than the Poisson law asks for,
and `yhat_lower` / `yhat_upper` come back NARROWER"). But the test — and the proof
obligation at `prophet-parity-v1.yaml` `poisson_sampler_domain` — bar only
`|mean − λ| ≤ 0.01·λ`.

**Concrete trigger.** `fn poisson(_rng: &mut Rng, lambda: f64) -> usize {
lambda.round() as usize }` — a sampler with *zero* variance, which would collapse every
logistic band to the deterministic trend — yields `rel = 0.0` at all six sweep points
and passes `poisson_mean_tracks_lambda_across_its_whole_domain` outright. It also passes
`wp_log_r_logistic_fixture_lambda_is_far_below_the_branch_threshold`, which touches the
fixture only. Whether the 32-rung parity ladder's `band_width_last30_rel` rung would
catch it at the fixture's λ = 3.12 is not established by anything in this diff; the test
that names itself the sampler's domain check does not.

**Fix.** Add a second assertion to the same sweep — the sample variance against λ, with
its own contract-owned tolerance:

```rust
let var = draws.iter().map(|&k| (k as f64 - mean).powi(2)).sum::<f64>() / (N_F - 1.0);
assert!((var - lambda).abs() / lambda <= var_bar,
        "poisson variance outside its domain at lambda={lambda:.1}: var={var:.4}");
```

`round()` inflates the variance by ≈ 1/12, negligible at every λ in the sweep, so a
1–2 % bar is real rather than a fudge.

### WR-03: the aggregate-dates refusal fires after the whole holiday loop, contradicting its own comment

**File:** `crates/aprender-forecast/src/forecast.rs:210-212` and `245-252`

**Issue.** The comment at line 210-211 says the running sum "is refused below the moment
the aggregate ceiling is passed". It is not: `holiday_dates_total` is only compared to
`MAX_HOLIDAY_DATES_TOTAL` at line 245, *after* the loop has finished parsing every date
of every holiday. The `holiday_columns` check two lines below the accumulation
(line 216) does exactly what this comment claims, which makes the inconsistency
deliberate-looking rather than obviously accidental.

**Concrete trigger.** 1 000 holidays, each `lower_window: 0, upper_window: 0` (so
`holiday_columns` reaches only 1 000 and never trips) and each carrying 1 000 dates:
`parse_date` runs ~1 000 000 times and ~1 000 `Vec<i64>` totalling ~8 MB are allocated
and pushed into `holidays` before the refusal at line 245 discards all of it. The
contract's own measurement puts the set-construction half of that at 18.049 ms; the
parse half is comparable. Amplification is roughly 1:1 with the (≈ 11 MB) payload, so
this is a hygiene defect rather than a second CR-01 — but it is unbounded work performed
after the door already knows the request will be refused, on the exact surface this
phase exists to bound.

**Fix.** Move the check into the loop, immediately after the accumulation, mirroring
`holiday_columns`:

```rust
holiday_dates_total += h.dates.len();
if holiday_dates_total > MAX_HOLIDAY_DATES_TOTAL {
    return Err(ForecastError::Validation(format!(
        "holidays carry more than max_holiday_dates_total {MAX_HOLIDAY_DATES_TOTAL} \
         dates in total; send fewer holidays or fewer dates per holiday"
    )));
}
```

(The message must drop the observed total, or keep the post-loop check as well, since
the in-loop count is a partial sum. Both refusals are cheap.)

### WR-04: the SC1 gate surface covers only the two shapes the phase chose; the harness 06-13 added for the third is `#[ignore]`d, has no recipe, no bar, and no `freq` knob

**File:** `justfile:799-847` (`forecast-holiday-bench`);
`crates/aprender-forecast/src/prophet.rs:1051-1113` (`logistic_band_wall`)

**Issue.** Two things in this diff assert an SC1 wall — `forecast-bench` (no-holiday) and
the new `forecast-holiday-bench` (holiday) — and both hard-code the geometry axis they
sweep. `logistic_band_wall`, the harness 06-13 added precisely because the fix increases
per-sample work, is `#[ignore]`d with no `just` recipe and no assertion, and it reads
only `LOGISTIC_BENCH_POINTS` / `LOGISTIC_BENCH_HORIZON` — never `freq`. Its four recorded
lines are all `freq: "D"`, the one value that does not exhibit CR-01 (measured 0.231 s
vs 2.334 s for `"MS"` at the same points and horizon). This is CLAUDE.md Verification
Discipline rule 5 in the concrete: the guard does not scan the surface where the cost is
decided, so nothing in the phase could have caught CR-01.

Separately, the recipe's own comment block asserts "**THE DEFAULTS ARE THE WORST SHAPE
THE DOOR STILL ACCEPTS, deliberately**" nine lines after stating that a 4 700-point /
5-column request — accepted, half the design-cost bound — "reproducibly walls at ~4.2 s".
Both cannot be true. The disclaimer above it ("NOT a general SC1 guarantee") is honest,
but the superlative is the sentence a future reader will quote.

**Fix.** Give `logistic_band_wall` a `LOGISTIC_BENCH_FREQ` knob and a
`just forecast-logistic-bench` recipe with the same 2 s bar and `profile=release` guard
as its holiday sibling; sweep `D`, `W` and `MS` at the tightest legal history span.
Reword the defaults claim to "the worst shape at the design-cost bound" — which is what
the three measured compositions actually support.

---

## Info

### IN-01: the `just` wall bar reads a non-numeric measurement as 0 and passes

**File:** `justfile:844`

`awk -v v="$total" 'BEGIN { exit (v + 0 < 2.0) ? 0 : 1 }'` — awk coerces a non-numeric
`v` to 0, so a token like `total_s=abc` prints `HOLIDAY DESIGN OK: abc s < 2.0 s (SC1)`.
The `-z "$total"` check above catches only an absent token, not a garbage one. The same
pattern is pre-existing at lines 620, 680 and 742, so this is consistency rather than a
new class — but the new recipe adds a fourth instance. Fix: validate the shape first,
e.g. `case "$total" in ''|*[!0-9.]*|*.*.*) echo "FAIL: total_s=$total is not a number"
>&2; exit 1;; esac`.

### IN-02: `feature_row`'s new `hol_sets` parameter is a breaking public-API change that trades a compile-time invariant for a runtime panic

**File:** `crates/aprender-forecast/src/prophet.rs:187-207`

`pub fn feature_row` gained a fourth parameter and now indexes `hol_sets[hi]`
(line 205) with an index derived from `Column.holiday`, i.e. from a *different*
argument. Passing `&[]`, or a set list built from a different `Spec`, is an
out-of-bounds panic in library code rather than a type error. `Design`'s fields are all
`pub`, so `d.spec.holidays.clear()` followed by `predict(&d, …)` reaches it. The
pre-change form indexed `spec.holidays[hi]` and had the same hazard, so this is not a
regression — but the change was an opportunity to close it. Consider returning
`(cols, hol_sets)` from one constructor, or `hol_sets.get(hi).is_some_and(|s| s.contains(…))`.
The signature change is also breaking for external callers of this 0.63.0 crate.

### IN-03: `cost_bounds_match_contract`'s doc says "four cost bounds"; the table has seven entries, one of which is not a cost bound

**File:** `crates/aprender-forecast/src/types.rs:228-266`

The doc comment ("The four cost bounds against the SAME contract") predates two
additions, and `DEFAULT_POOL` — a router-pool size, not a per-request cost ceiling — now
rides in the same table. The assertion itself is correct and all three new constants
*are* covered; only the narration drifted. Update the count and either split
`DEFAULT_POOL` into its own assertion or widen the doc to "contract-mirrored constants".

### IN-04: five source files changed in the reviewed range fall outside the stated 9-file scope

**Files:** `crates/aprender-forecast/src/bolt.rs`, `crates/aprender-forecast/src/dates.rs`,
`crates/aprender-forecast/src/safetensors.rs`, `crates/aprender-mcp-chronos/src/lib.rs`,
`crates/aprender-mcp-forecast/src/main.rs`

`git diff ce3e5a8ea..HEAD` touches 14 source files, not 9. I read the extra five; two
are worth recording rather than dropping:

- `bolt.rs:275-276` replaces a hand-written transpose loop with
  `trueno::blis::transpose(out, inp, w, &mut t).expect("… guaranteed by every caller")`.
  The safety argument (`w.len() == out * inp`) is enforced by no type and by no
  assertion at the call sites; a caller that violates it now panics in a library instead
  of writing a partial result. A `Result` return, or a `debug_assert_eq!(w.len(), out * inp)`,
  would make the claim checkable.
- `safetensors.rs` deletes `pub fn load(path: &str)`. I found no remaining callers in
  `crates/` or `src/`, so the deletion is correct as dead code — but it is a breaking
  removal from a published crate's public surface and belongs in a semver note.

`dates.rs`'s `parse_date` rewrite (byte fold in place of `split('-').parse()`) is
sound: the shape gate at lines 79-91 proves ten ASCII bytes, `-` at 4 and 7, and digits
elsewhere before the fold runs, so the removed error arm really was unreachable, and the
4-digit year keeps the day count far from any i64 edge.

---

## Verification notes

- `cargo test -p aprender-forecast --lib -- types::tests::cost_bounds_match_contract
  prophet::sampler forecast::tests` → **16 passed, 0 failed, 1 ignored**.
- CR-01's A/B used two release binaries of `aprender-mcp-forecast`: HEAD, and one built
  from a detached worktree at `ce3e5a8ea` (removed afterwards; no repo source was
  modified). Both were driven over the shipped streamable-HTTP `/mcp` route with
  `--pool 1`, so the numbers are the served surface, not a libtest harness.
- WR-01's sweep was reproduced in a standalone scratch binary that replicates
  `prophet::Rng` and the normal branch verbatim, not by editing the crate.

---

_Reviewed: 2026-09-06_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard, incremental over `ce3e5a8ea1ecbb3d873ce81972e099f8b4391758`_
