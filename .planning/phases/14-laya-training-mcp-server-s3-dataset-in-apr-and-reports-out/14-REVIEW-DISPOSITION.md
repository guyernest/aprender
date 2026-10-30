---
phase: 14
review: 14-REVIEW.md
recorded: 2026-10-29
---

# Phase 14 review disposition

Advisory review (standard depth): 0 critical, 4 warning, 3 info. Nothing is fixed yet; every finding is `open`
until the owner triages it. WR-01 and WR-02 are real behaviour gaps that no live run exercised.

| Finding | Severity | Disposition | Note |
|---------|----------|-------------|------|
| WR-01 | warning | open | `finalize_run` refused/crash/timeout classification never runs on the normal path (the CDK Catch always passes `error`, so `stage_error` returns before `outcome.json` is read); `prefilter_disagreement` never set, the trainer's `REFUSED` line never reaches the caller. Live runs only covered the pack-refusal path. |
| WR-02 | warning | open | Finalization can wedge: manifests list `outcome.json` / `pack-outcome.json` before upload; a failed upload makes tagging error forever, lock held, 10-minute sweep keeps failing past the 11 h lease. |
| WR-03 | warning | open | Raw AWS error text (account ids, role ARNs, ECR URIs) can reach API callers and the stored status payload. |
| WR-04 | warning | open | Finalizer Lambda asset has no provenance/freshness check at deploy time (only existence); images are built from a clean `git archive HEAD`. Related: W-3 in 14-SECURITY.md. |
| IN-01 | info | open | `training-manifest.json`, `partial-manifest.json`, `pack-manifest.json` keep the `expire` tag. |
| IN-02 | info | open | Immutable commit tag: rebuilding the same HEAD after a partial build fails at `docker push`. |
| IN-03 | info | open | Pre-filter and trainer gate-contract hashes recorded but never compared. |
