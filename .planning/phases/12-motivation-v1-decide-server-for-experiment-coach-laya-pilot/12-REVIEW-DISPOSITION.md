---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
review: 12-REVIEW.md
recorded: 2026-10-26
---

# Phase 12 review disposition

One row per finding in 12-REVIEW.md. `open` means seen and not yet decided; nothing here was fixed
by the orchestrator. No Critical finding was raised.

| Finding | Severity | Disposition | Note |
|---------|----------|-------------|------|
| WR-01 | warning | open | A pilot waiver has no ceiling on ece_post; unused in this phase (the declared run passed) |
| WR-02 | warning | open | Waiver not bound to one attempt (applies_to carries recipe_id, not eval_probs_sha256) |
| WR-03 | warning | open | `apr eval --dump-probabilities` write can fail after the one-shot test pass |
| WR-04 | warning | open | Bake-off does not compare eval/zero-shot probability hashes or the artifact sha to the gate report |
| WR-05 | warning | open | `blocked:<class>:<path>` accepts a missing evidence file (evidence_sha256 null) |
| WR-06 | warning | open | Served door of 8 is overstated for the 97-token task; the hand-off discloses the effective door |
| IN-01 | info | open | Python waivable set from YAML, Rust hard-codes ece_post |
| IN-02 | info | open | FALSIFY-LAYA-GATE-020 prediction text is stale |
| IN-03 | info | open | GUARD_DEFAULT in probe_epoch.py is dead |
| IN-04 | info | open | Two justfile inaccuracies |
| IN-05 | info | open | Length-band cut points depend on float noise in 100/3 |
