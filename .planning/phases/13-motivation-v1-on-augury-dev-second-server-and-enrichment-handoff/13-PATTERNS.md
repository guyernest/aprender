# Phase 13: motivation-v1 on augury-dev - Pattern Map

**Mapped:** 2026-10-27
**Files analyzed:** 12 new/modified
**Analogs found:** 11 / 12 (one partial)

Mapped against the WORKING-TREE justfile (uncommitted `region` diff, 18 insertions / 7 deletions, see below).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `justfile` `laya-deploy-config` (commit `region` param) | config/recipe | transform | itself (working tree, lines 1718-1763) | exact |
| `justfile` `laya-weights-bucket` (commit `region` param) | recipe | request-response (AWS) | itself (lines 2742-2772) | exact |
| `justfile` `laya-deploy-selftest` (extend: region cases, re-mutation) | test harness | batch, AWS CALLS 0 | itself (lines 2256-) | exact |
| `scripts/laya_gates.tsv` (new gate row(s) for region) | config/gate register | batch | row 8 `deploy-selftest-skip` | exact |
| `justfile` `laya-deploy` / `laya-deploy-verify` / `_laya-edge-health-get` (target-explicit, bounded first-health retry, bearer) | recipe | request-response | themselves (1947, 2996, 2152) | exact |
| `crates/aprender-mcp-decide-lambda/examples/probe.rs` + `src/probe.rs` (bearer path, unauthenticated refusal) | utility/CLI | request-response | itself: bearer already exists | exact (see note) |
| `scripts/laya_train/prepare_motivation.py` (`--enrichment`, repeatable) | ingest | file-I/O, transform | itself (`build`, `check_pin`, declared rules, selftest) | exact |
| `scripts/laya_train/data.py` (reuse only, no change expected) | utility | transform | `normalize`, `normalized_sha256`, `group_train`, `refuse_overlap` | exact |
| enrichment format `motivation-enrichment-v1` | schema | file-I/O | `contracts/apr-book-ch21-v1.yaml` (`metadata.kind: schema`) | partial |
| `13-*-EVIDENCE.json` (probe, deploy, acceptance, region) | evidence | n/a | `12-DEPLOY-EVIDENCE.json`, `12-ACCEPTANCE-REPLAY.json` | exact |
| `13-HANDOFF.md` | doc | n/a | `12-HANDOFF.md` | exact |
| selection note (record schema + sampling) | doc | n/a | `12-HANDOFF.md` sections 4, 6 style | role-match |

## The uncommitted justfile diff (what this phase commits)

`git diff justfile`: three hunks, nothing else.

1. `laya-deploy-config apr auth server env="dev" profile="ze-kasher-dev" region="us-east-1":` (line 1718). The python heredoc gets `"{{region}}"` as a 7th argv (`template, out, server, uri, sha, auth, region = sys.argv[1:8]`) and:
```python
region_line = 'region = "us-east-1"'
if text.count(region_line) != 1:
    sys.exit(f"{template}: expected exactly one '{region_line}' line; restore the template")
text = text.replace(region_line, f'region = "{region}"')
```
2. `laya-weights-bucket ... region="us-east-1"` (line 2742): `R="{{region}}"`; create-bucket takes no LocationConstraint for us-east-1, otherwise `--create-bucket-configuration "LocationConstraint=$R"`; final echo prints `(region $R)`.
3. The zero-or-two-lines refusal is the `!= 1` check above; D-20 needs a selftest case for it.

Note: the diff does not touch `laya-deploy`, `laya-deploy-verify` or `laya-upload`; they take `profile` but no region, so D-06's "every recipe told which target" is still to do. Also `laya-weights-bucket` only passes `--region` on create; head-bucket and the put-* calls rely on the profile's default region.

## Pattern Assignments

### `laya-deploy-selftest` (justfile 2256-), extend for region (D-20)

Analog: itself. Reuse the harness pieces:
- Offline guarantee: `export DRY_RUN=1`, aws recorder shim first on PATH exiting 97 (lines 2298-2309), with a positive control proving it records, then `AWS CALLS` counted at the end (must stay 0).
- One server variable: `SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"`, `CFG="models/decide/deploy-config-$SRV.toml"`; backup and restore on EXIT (lines 2276-2284).
- Config regeneration helper: `regen() { just laya-deploy-config "$A" off "$SRV" > "$CASES/config-$1.log" 2>&1 || {...}; }` (2313). A region case calls `just laya-deploy-config "$A" off "$SRV" dev ze-kasher-dev eu-west-1` (positional: apr auth server env profile region) and greps `region = "eu-west-1"` in `$CFG`.
- Refusal cases: `expect_refused N name 'regex' cmd...` (2317-): non-zero AND the expected reason or FAIL. Use it for the zero-line and two-line template cases (mutate a copy of the template; do not edit the tracked one).
- Byte-identity for the default: record `sha_or_absent`/`shasum -a 256` of the us-east-1 `$CFG` before and after; D-20 says default output is byte-identical to today's, so pin the pre-change sha from `git stash`/HEAD render, not from the new code.
- Shared-root guard: `root_digest` over `crates/.pmcp/*` (2287-2297) is the existing mechanism to prove local cargo-pmcp state is untouched; the D-06 two-deployment check should extend `ROOT_BEFORE`/`ROOT_AFTER` comparison for the eu-west-1 render.
- CLAUDE.md verification rule 4: re-run the RED-turning mutation in the new scope (e.g. break the `replace` line, expect the new case to FAIL). Register it in `scripts/laya_gates.tsv`.

### `scripts/laya_gates.tsv` (gate register)

Header (tab-separated): `gate_id	target	must_fail	must_pass	finding`. 38 lines; add rows in this shape. Closest analog row 8:
```
deploy-selftest-skip	just laya-deploy-selftest	armed with the synthetic fixture (...) -> exit 1 "FAIL positive dry run"	disarmed (...) -> exit 0 ending "DEPLOY SELFTEST OK (positive dry run SKIPPED: ...)", never the bare OK; default-armed ... -> DRY-RUN OK and the bare OK	AL7
```
New rows: `deploy-region` (must_fail: template with 0 or 2 `region = "us-east-1"` lines; render with region eu-west-1 lacking `region = "eu-west-1"`; must_pass: eu-west-1 render, default render byte-identical). Check for a test that reads this TSV before adding, since gate ids are mirrored elsewhere (grep `laya_gates.tsv` in scripts/tests).

### `_laya-edge-health-get` (justfile 2152-) and `laya-deploy` / `laya-deploy-verify`

Analog: itself. Env-tunable bounded retry already exists:
```bash
TRIES="${LAYA_EDGE_HEALTH_TRIES:-40}"; SLEEP="${LAYA_EDGE_HEALTH_SLEEP:-15}"; MAX="${LAYA_EDGE_HEALTH_MAX_SECONDS:-900}"
for name in TRIES SLEEP MAX; do ... case "$value" in ''|*[!0-9]*) refuse_config ...
```
with `refuse_config() { rm -f "$OUT"; echo "REFUSED edge-health-get: config $1" >&2; exit 2; }`. F-10-12-1 (first-health 404 on a new tenant) is therefore already a bounded retry; the plan should pass the augury health URL through it and record try counts, not add a new loop. `laya-deploy apr run data base server env profile` (1947) and `laya-deploy-verify apr server profile samples` (2996) take `profile` but no target/region/URL; add an explicit target argument rather than an ambient `PMCP_TARGET`.

### `probe.rs` bearer path (D-07)

Correction to CONTEXT: the probe already has a bearer path. `crates/aprender-mcp-decide-lambda/examples/probe.rs`:
- Header doc line 43: "The bearer token (`--bearer` or `APRENDER_DECIDE_PROBE_TOKEN`) is never printed."
- `bearer: Option<String>` (88); parse `"--bearer" => bearer = Some(value("--bearer")?)` (128); env fallback `bearer.or_else(|| std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty()))` (182); passed as `args.bearer.as_deref()` (232).
- Args use `args_os` so a non-UTF-8 arg cannot panic and the arg is never quoted in errors (107-110).
- `src/probe.rs`: `Rpc::new(url, bearer, probe_id)` (159) and `if let Some(token) = self.bearer { req = req.bearer_auth(token); }` (193-194).

What is genuinely missing for D-07: (a) a FILE form (`--bearer-file PATH`, since the token must not be on argv; `--bearer` puts it on argv, so keep it but prefer the file; mirror the empty-filter and never-printed rules), (b) client-credentials token exchange from an M2M secret file (or have the owner supply a token file), and (c) an "unauthenticated call is REFUSED" probe mode that asserts 401/403 and prints only the status, in the style of `--replay-over-limit` (passes only when the server refuses). The justfile `laya-deploy-verify` has no bearer plumbing (grep for bearer/TOKEN in the justfile finds no matches relevant to it), so thread the file path through there too. Add it as a flag in the existing `parse_args` match, not a new binary.

### `scripts/laya_train/prepare_motivation.py` (`--enrichment`, D-09..D-12, D-15, D-16)

Analog: itself (556 lines). Patterns to extend, not copy:
- Declared rules block in the module docstring (lines ~20-45): "DECLARED RULES (written before the run; the run only applies them)", with `RULE conflicting-labels`, `RULE split`, `no shift`. Add `RULE enrichment-*` entries here BEFORE code: train-only (D-11), drop-on-eval-match counted, cross-source conflict drops the whole group with no precedence (D-12), weak/mixed -> `other` (D-15, guide 10.3), `annotation_mode` recorded and counted not acted on (D-16).
- Constants: `LABELS`, `LABEL_OF`, `SPLIT_SEED = 13`, `SENTINEL = "ZQXJ-sentinel-motivation-7mw"`, `PINNED = {...}`.
- Grouping, the core to extend (`build`, lines 166-208):
```python
groups = {}   # normalized hash -> [first exact text, [labels]]
for _, text, label in rows:
    g = groups.setdefault(normalized_sha256(text), [text, []])
    g[1].append(LABEL_OF[label])
mixed = [g for g in groups.values() if len(set(g[1])) > 1]
kept = [(g[0], g[1][0]) for g in groups.values() if len(set(g[1])) == 1]
```
  Enriched rows join the SAME `groups` dict (so cross-source conflicts reuse the rule), but they must never enter the eval draw: compute the eval split from the partner-file groups exactly as today (`np.random.RandomState(SPLIT_SEED)` over partner-only `kept`), then append surviving enriched groups to `train_rows` only; drop any enriched group whose key is in the eval hash set and count it. D-10 follows only if the no-`--enrichment` path executes the same statements in the same order; add a check that sha256s `task.json` (f74a3ad3...), `train.jsonl`, `eval.jsonl` against v1's bytes.
- Validators after assembly (keep): `group_train(train_rows)`, `refuse_overlap(train_rows, eval_rows)`, `calibration_split(...)`.
- Pin check shape: `check_pin(summary)` raises `PrepareFailed` naming differing KEYS only. Enrichment sha256 and counts are PRINTED and pinned in the output (not hardcoded in `PINNED`, since no real file exists).
- NO TEXT rule (docstring): refusals name a 1-based row number, a field and at most a character count, never a value; malformed header reported by column count. Reuse this for every enrichment validator (unknown evidence level, missing field, bad label).
- Counts: summary dict style (`rows_in`, `mixed_label_groups_dropped`, ...); add `enrichment: {sha256, rows_in, per_label, per_evidence_level, per_(label,evidence) cell, annotation_mode counts, dropped_eval_overlap, dropped_conflict_groups}`.
- Selftest: `_synthetic_csv(path, header=None)` (line 256) and `SENTINEL` are the planted-sentinel pattern. Add a synthetic enrichment fixture carrying the sentinel in a text and in a deliberately bad field, and assert the sentinel is absent from all stdout/stderr and from refusal messages. Read the selftest section of the file (after line 256) before writing it, since I mapped its helpers by name only.
- Write-once output via `prepare_stance.write_once` (re-run is a no-op when bytes match).
- Invocation: `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py` (docstring line 5); default CSV under `~/projects/mcp/motivation-experiment/datasets/motivation/`, default out `data/decide/motivation-v1` (gitignored `/data/`).

### `data.py` (reuse only)

`normalize` (61), `normalized_sha256` (76), `exact_sha256` (80), `load_rows` (172), `refuse_unencodable` (161), `group_train` (263), `refuse_overlap` (222), `calibration_split` (283). The ONE normalizer is nfc-trim-ws-v1; do not add another. The enrichment text identity must call `normalized_sha256`.

### Enrichment format `motivation-enrichment-v1` (D-13)

Analog: `contracts/apr-book-ch21-v1.yaml` has `metadata.kind: schema` and `contract:` header, but these are apr-book page contracts and their `pv validate` acceptance is unverified: `pv` is not on PATH in this shell (`command not found`), so I could not run it. Planner action: build/locate `pv` (`crates/aprender-contracts-cli`), run `pv validate` on a candidate; if rejected, fall back per CLAUDE.md to a schema document (e.g. under `docs/` or beside `scripts/laya_train/`) plus the ingest's validator, never a bash re-implementation. Do not place it in `contracts/` unless `pv validate` passes (and note monorepo counts such as README contract count are an equality gate in `readme_contract.rs`; adding a contract file changes `find contracts -name '*.yaml' | wc -l`).

Fields to declare (planner's discretion): `format` version, `annotation_mode`, per-row `post_id`, `source`, `text`, `label`, `evidence_level` (clear/weak/mixed), plus the D-14 record carry-through (`labels`, `probabilities`, `artifact_sha256`, deployment triple, `tokens`, `truncated`, `timestamp`).

### Evidence JSON shapes (Phase 12)

`12-DEPLOY-EVIDENCE.json` top-level keys (verified): `schema, phase, plan, server, profile, region, env, auth, tier_mb, artifact_path, run_dir, gate_final_attempt, artifact_sha256, waived, pilot_label, pilot_label_note, d23, config, selftest, upload, antisemite_before, live_attempts, live_function_after, findings, measured_on, commit, token_gate_line, owner_login_reply, antisemite_after, served_path`. Copy the skeleton for `13-DEPLOY-EVIDENCE.json` and ADD the D-06 triple as explicit keys (`pmcp_target`, `aws_account`, `region`), `ze_kasher_before/after` (sha d202f495 read-only recheck, mirroring `antisemite_before/after`), `auth_enabled_at`/publish-to-enable interval (D-07), and `unauthenticated_refused`. Never store tokens or secrets. Other 12 files to mirror by name: `12-PROBE-EVIDENCE.json` (read-only probe, D-18), `12-ACCEPTANCE-REPLAY.json` (replay shape), `12-DEPLOY-OWNER-REPLY.json` (201 B, owner login checkpoint reply), `10-RENAME-DEPLOY-EVIDENCE.json` for F-10-12-1..3.

### `13-HANDOFF.md`

Analog `12-HANDOFF.md` headings: `## 2. Identity (SC-3)`, `## 3. Endpoint and auth posture`, `## 4. Items for the partner to confirm`, `## 5. The task and the limits as deployed`, `## 10. Acceptance replay and timings (reported, not gated)`, `## 11. Scope and deferred`, `## 12. Dates and conditions`, `## Numbers and their records`. Keep identity/endpoint/auth/partner-items/replay/scope/dates; drop bake-off and prior sections; add: the triple, M2M details by out-of-band only, `motivationUrl`/`motivationSha`/`MotivationAuthSecretArn` items (D-08), the enrichment format, the selection note (D-17) and the anchoring warning (D-16), and an explicit statement that the ingest is proven on fixtures only (D-09).

## Shared Patterns

- **Target/triple explicitness (D-06):** every recipe takes the profile (and now region) as an argument; the shared deploy root state `crates/.pmcp/{deploy.toml,deployment.toml,active-target,deploy}` is digested by `root_digest()` in the selftest. Reuse it for a before/after proof around the augury publish.
- **Offline-first, recorder-proven zero AWS calls** for anything labelled selftest (2298-2309).
- **No-text/no-secret output:** counts, sha256s, row numbers only (prepare_motivation docstring); the probe never prints the bearer (probe.rs header 43).
- **Declared before run; a refusal is a result:** docstring rules precede code; `expect_refused` demands both non-zero and the expected reason.
- **Read-only checks before live steps:** `aws sts get-caller-identity --profile P --query Account --output text` is how `laya-weights-bucket` derives the account (2752); the bucket name is `aprender-decide-weights-${ACCOUNT}-${ENV}`.
- **zsh caveat:** the Bash tool runs zsh (memory), so selftest case tables must run under `#!/usr/bin/env bash` recipes as today.

## No Analog Found

| File | Role | Reason |
|---|---|---|
| unauthenticated-refusal probe mode and client-credentials token acquisition | CLI | probe has a bearer header but no file-form or refusal assertion; model the refusal assertion on `--replay-over-limit` |
| enrichment schema location | schema | only `kind: schema` analogs are unrelated apr-book contracts; `pv` acceptance unverified |

## Metadata

**Analog search scope:** `justfile` (working tree), `scripts/laya_gates.tsv`, `scripts/laya_train/`, `crates/aprender-mcp-decide-lambda/{examples,src}/probe.rs`, `contracts/`, Phase 10/12 evidence and hand-off files.
**Limits:** the justfile was read at lines 1718-1763 (via diff), 2152-2176, 2256-2325, 2742-2772 only; the middle of `laya-deploy-selftest`, `laya-deploy` and `laya-deploy-verify` bodies, and `prepare_motivation.py` after line 256, were not read. `pv` was not runnable here. Tracked-source gate: all cited paths are repo-tracked sources, none are `.gsd` mirrors (not individually `git ls-files` checked).
**Pattern extraction date:** 2026-10-27
