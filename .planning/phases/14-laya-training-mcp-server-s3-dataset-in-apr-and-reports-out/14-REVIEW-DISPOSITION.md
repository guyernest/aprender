---
phase: 14
review: 14-REVIEW.md
recorded: 2026-10-30
---

# Phase 14 review disposition

Advisory re-review of the gap round 14-16..14-22 (standard depth, diff base 2ead5836f): 0 critical, 3 warning,
9 info. Every finding below is `open` until the owner triages it. The orchestrator checked the three warnings
against the code before recording them: WR-01 is confirmed by reading (`finalize.rs` `clip` truncates before
`StatusPayload::to_value` redacts); WR-02 is a latent hazard that did NOT engage in the 14-22 deploy
(`.cargo/config.toml` deliberately sets no `target-dir`, `CARGO_TARGET_DIR` is unset, and the fixed-path binary
hashes to the deployed `fc796b8b…`); WR-03 is a real scope gap (14-19 attested only the finalizer).

| Finding | Severity | Disposition | Note |
|---------|----------|-------------|------|
| WR-01 | warning | open | Job-written text is clipped to 1024 characters before redaction, so an account id or AKIA key straddling the cut survives as an unrecognized fragment. Fix: clip through `caller_safe(text, MAX_ERROR_CHARS)`. |
| WR-02 | warning | open | `laya-trainer-synth` copies the finalizer from a hard-coded `target/` path, so a redirected target dir would let a stale binary be attested as built from HEAD. Not engaged today. Fix: take the path from `cargo metadata` or the compiler-artifact message. |
| WR-03 | warning | open | The request function (now the caller-facing WR-03 boundary) is deployed from the working tree by `cmd_deploy` with no clean-closure check and no build record. Fix: reuse the 14-19 provenance rule for that binary. |
| IN-01 | info | open | Override note always says "train job"; Train exit 1 after a recorded pass is reported as the pack stage leaving no record. |
| IN-02 | info | open | Results stored before the 14-18 redeploy are still served unredacted by `tasks/result` and copied into `verdict.json`; `train_status` re-redacts; ends at task TTL. |
| IN-03 | info | open | Redaction blind spots: hyphenated account form, 12 digits next to a letter, AROA/AIDA ids, swallowed colon when the error name is redacted. Covered in kind by AR-6. |
| IN-04 | info | open | Carried prior IN-02: immutable commit tag blocks rebuilding the same HEAD after a partial build. |
| IN-05 | info | open | Carried prior IN-03: pre-filter and trainer gate-contract hashes recorded but never compared. |
| IN-06 | info | open | `_laya-crates-root-swap` labelled "decide config" during the laya deploy (relabel together with the selftest fixture). |
| IN-07 | info | open | No pmcp.run login precheck before the grant revoke (14-22 B2 attempt 1). |
| IN-08 | info | open | Drift script: no total task cap, no stop-task on timeout (accepted AR-7). |
| IN-09 | info | open | Real account ids in the committed justfile target table (W-5, predates this round). |

## Prior round (14-REVIEW.md of 2026-10-29), closed by the gap round

The prior ids WR-01..WR-04 and IN-01..IN-03 are re-used by the new review for different findings, so the prior
rows are kept here as prose rather than as ledger rows.

- prior WR-01 (Catch stage error read before outcome.json): fixed in 14-17 (`finalize.rs` outcome-first), re-review confirms.
- prior WR-02 (listed-but-absent object wedges tagging and the lock): fixed in 14-17 (`drop_absent_entries`), re-review confirms.
- prior WR-03 (raw AWS text reaches callers): fixed in 14-18 at three boundaries; the residual is the new WR-01 and IN-03.
- prior WR-04 (finalizer deployed without provenance): fixed in 14-19 for the finalizer; residuals are the new WR-02 and WR-03.
- prior IN-01 (self-listing manifests keep the expire tag): fixed in 14-17, re-review confirms.
- prior IN-02, IN-03: still open, carried as the new IN-04 and IN-05.
