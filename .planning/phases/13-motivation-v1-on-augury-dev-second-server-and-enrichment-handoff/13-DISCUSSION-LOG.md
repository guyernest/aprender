# Phase 13: motivation-v1 on augury-dev (second server) and the first enrichment hand-off - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-27
**Phase:** 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
**Areas discussed:** Server id & auth, Enrichment ingest & split, Record schema & selection, augury deploy path

Scout findings presented before the questions: augury-dev is a separate pmcp.run tenant (`<server>.eu-west.augury.pmcp.run`, counter-coach live there); the partner expects auth-protected classifiers on augury (`protectedClassifiers=true`, an M2M secret per classifier); the partner's augury context names an antisemite server on augury that this repo has not deployed.

---

## Server id & auth

| Option | Description | Selected |
|--------|-------------|----------|
| -motivation-augury | New id and thin crate, register +1 (roadmap proposal) | |
| Reuse -motivation | Same id on the augury tenant; no new crate; amends the roadmap | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Auth on, partner runbook | Publish auth off, owner enables OAuth in the admin UI, M2M client; authenticated replay plus unauthenticated refusal | ✓ |
| Auth off this round | As in Phases 10/12 | |

| Option | Description | Selected |
|--------|-------------|----------|
| Hand-off only | URL, sha and M2M details to the owner; no partner-repo or stack writes | ✓ |
| This phase writes the secret | Executor writes the augury Secrets Manager secret | |

**Notes:** Reusing the id amends the roadmap's "own thin crate, register grows by one"; the deployments are told apart by (target, account, region), and the planner must keep cargo-pmcp's local state for the two apart.

---

## Enrichment ingest & split

| Option | Description | Selected |
|--------|-------------|----------|
| Implement + selftest | `--enrichment PATH`, pins, no-text, sentinel selftest on fixtures | ✓ |
| Specify format only | Spec plus validator stub; ingest at retrain | |

| Option | Description | Selected |
|--------|-------------|----------|
| Train only, eval frozen | eval.jsonl byte-identical; eval-matching enriched texts dropped | ✓ |
| Re-split the union | Bigger eval, not comparable with v1 | |
| Frozen eval + new eval2 | Second field-drawn eval held out from the enrichment | |

| Option | Description | Selected |
|--------|-------------|----------|
| Drop group, count it | Existing conflicting-labels rule across sources | ✓ |
| Partner file wins | | |
| Expert (enrichment) wins | | |

**Notes:** Claude added D-10 (a run without enrichment is byte-identical to v1, task.json sha f74a3ad3… included).

---

## Record schema & selection

| Option | Description | Selected |
|--------|-------------|----------|
| Text included, partner side | Record carries text; lives only in the partner repo | ✓ |
| Id only, text joined later | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Label + evidence level | Three labels plus clear/weak/mixed | ✓ |
| Label only | | |
| Label + free-text note | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Stratified quota + uncertainty band | Quota by predicted label (C over-sampled), uncertainty band, random control | ✓ |
| Uncertainty-first | | |
| Pure random + C over-sample | | |

| Option | Description | Selected |
|--------|-------------|----------|
| Blind (Recommended) | Expert sees post_id and text only | |
| Shown | Expert sees the prediction | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| ~150 posts (Recommended) | | |
| ~300 posts | | ✓ |
| Leave it to the partner | | |

**Notes:** The owner chose the prediction shown against the recommendation; recorded as `annotation_mode: prediction_shown` (D-16), carried for the retrain's hand-off.

---

## augury deploy path

| Option | Description | Selected |
|--------|-------------|----------|
| Read-only probe, then go | sts, cargo-pmcp target, counter-coach endpoint; deploy without extra checkpoint | ✓ |
| Owner checkpoint before deploy | | |

| Option | Description | Selected |
|--------|-------------|----------|
| New bucket in augury eu-west-1 | | |
| Cross-account read from ze-kasher-dev | | |
| Other (free text) | "Reuse the S3 bucket already in augury-dev eu-west-1 used for the antisemite weights; create a new one if access fails; think about scale with multiple models sharing the bucket" | ✓ |

| Option | Description | Selected |
|--------|-------------|----------|
| Selftest both regions | eu-west-1 rendered, us-east-1 byte-identical, 0/2 region lines refused, re-mutation | ✓ |
| us-east byte-identity only | | |

**Notes:** Verified read-only during the discussion: `aprender-decide-weights-783960389328-dev` is in eu-west-1 and holds `decide/aprender-mcp-decide-antisemite/2293bebb….apr` (2026-10-24). The per-account bucket, per-server prefix and per-prefix grant layout already scales to more models.

---

## Claude's Discretion

- Field names of the record and enrichment schemas, and where the format lives.
- The uncertainty-band definition and quota proportions.
- The credential file form for the authenticated replay and the unauthenticated-refusal probe.
- The mechanism keeping the two deployments' local cargo-pmcp state apart.
- Evidence file names.

## Deferred Ideas

- motivation-v2 retrain on the enrichment; a field-drawn eval2; blind annotation; antisemitism-v1 on augury; auth on for ze-kasher-dev servers; the experiment-coach motivation trio (partner work).
