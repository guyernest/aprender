# Phase 5 — Deferred Items (out-of-scope discoveries)

Discoveries made while executing Phase 5 plans that are NOT caused by this phase's
changes. Logged rather than fixed, per the executor scope boundary.

---

## D-ITEM-05-01: `aprender-serve` test target `driver_cpu` does not compile

**Found during:** plan 05-04, Task 3 (`cargo check --workspace --all-targets` run to prove
the new `AprenderError::ZeroVarianceDifferences` variant broke no downstream match).

**Symptom:** 12 × `E0063` in `crates/aprender-serve/tests/driver_cpu.rs`:

```
error[E0063]: missing field `query_pre_attn_scalar` in initializer of `GGUFConfig`   (×10)
error[E0063]: missing fields `post_attn_norm_weight` and `post_ffw_norm_weight`
              in initializer of `OwnedQuantizedLayer`                                (×2)
error: could not compile `aprender-serve` (test "driver_cpu") due to 12 previous errors
```

**Proven pre-existing, not caused by 05-04:**

- `query_pre_attn_scalar` was added to `GGUFConfig` on 2026-06-19 in `366f3c275`
  (*fix(serve): batched-GPU path crashed on every GQA model*, PMAT-841, #2125), which is
  an ancestor of this plan's base `350b08575`. The test target has been red since then.
- The workspace check output contains **zero** occurrences of `AprenderError`,
  `non-exhaustive` or `ZeroVarianceDifferences`, so the added error variant is not
  implicated. Adding it broke nothing: only `error.rs`'s own `Display` match is
  exhaustive over that enum.

**Why not fixed here:** it is in a different crate, on a surface plan 05-04 does not
touch, and the fix is to update a fixture-construction site for two struct fields added
by a GQA dispatch change — an `aprender-serve` concern with its own review context.

**Suggested owner:** an `aprender-serve` ticket. Note that `make tier2`/`tier3` and the
`workspace-test` required status check may already be masking this by not building
`--all-targets` for that crate; if so, that gap is the more important half of the fix
(CR-02: a gate that runs nothing passes).
