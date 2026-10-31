# Phase 15: Laya pack-bound amendment (D-15) and motivation-v1 re-reproduction (D-16) - Pattern Map

**Mapped:** 2026-10-31
**Files analyzed:** 13 new/modified
**Analogs found:** 13 / 13 (all tracked in git; verified with `git ls-files`)

Note: line numbers are from HEAD at mapping time. Tracked-source gate satisfied: no gitignored mirror paths are named. Phase-14 evidence files are tracked; `models/decide/*` and finalizer `bootstrap` are gitignored and are NOT analogs.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `contracts/laya-parity-v1.yaml` | config (provable contract) | transform (policy source) | itself, amendment A1 (lines 44-110, 179-190) | exact |
| `contracts/decide-apr-v1.yaml` (~388 text) | config | text only | A1 prose in `laya-parity-v1.yaml` | role-match |
| `contracts/aprender/binding.yaml` (~2330-2345 notes) | config | text only | same | role-match |
| `crates/aprender-decide/src/verify.rs` | service (verifier) | request-response / transform | itself: `ParityConstantsView` 474-480, `VerifyPolicy` 98, `rescore_bounds` 2394-2450 | exact |
| `crates/aprender-decide/src/verify/tests.rs` | test | transform | `write_noise_record` 1664-1700, `noise_*` tests (1751+) | exact |
| `crates/aprender-decide/examples/pack_laya.rs` | utility (CLI) | batch | itself (doc comment only) | exact |
| `justfile` recipes `laya-deploy` (step 5, ~2516-2533), `laya-upload` (~4084-4100), `laya-verify` (1765) | config (recipe) | request-response | the current step 5 / upload eligibility block | exact |
| `justfile` `laya-deploy-selftest` (3470) + `scripts/laya_gates.tsv` rows | test (gate table) | event-driven | existing `deploy-refusals`, `upload-eligibility`, `verify`, `deploy-selftest-skip` rows | exact |
| `CLAUDE.md` Decision-model sentence on the bar | config (doc) | text | the A1 sentence ("max(1e-5, 4 x ...) amendment A1") | exact |
| `15-*-OWNER-DECISIONS.md` (checkpoint record) | doc | record | `14-GAP-ROUND-2-OWNER-DECISIONS.md` (table of Q / verbatim owner answer + consequences) | exact |
| `15-REPRODUCTION-DECLARATION.json` | config (declaration) | record | `14-REPRODUCTION-DECLARATION.json` | exact |
| `15-GATE-RUN-EVIDENCE.json` | evidence | record | `14-GATE-RUN-EVIDENCE.json` (keys: schema, attempts, verdict, next_trusted, claim_scope, owner_replies, declaration_commit, ...) | exact |
| `15-DRIFT-EVIDENCE.json` (re-pack, D-15) | evidence | batch | `14-DRIFT-EVIDENCE.json` via `scripts/laya_deploy/laya_drift.py aws-plan/collect` | role-match (write a NEW file; `collect` merges into 14's) |

## Pattern Assignments

### `contracts/laya-parity-v1.yaml` (config, policy source)

**Analog:** itself. Add A2 beside A1; flat scalars only (file header rule, ~line 80).

**Existing A1 constants to leave unchanged** (lines 94-104):
```yaml
  pack_rescore_noise_k: 4
  pack_rescore_bound_max_abs: 1.0e-3
```
Add new flat keys next to them (Option S-B, RESEARCH Pattern 2): `pack_rescore_noise_k_fine_tuned`, `pack_rescore_floor_fine_tuned`, `pack_rescore_bound_max_abs_fine_tuned`, and `_zero_shot` twins. Values come ONLY from the recorded owner sign-off. Copy the comment style of lines 95-103 (rationale + "planner's declared value, flagged for review; approved <date>"). Add an `AMENDMENT (A2, ...)` paragraph after the A1 paragraph (lines 48-65 style: dated, user decision, declared before the run, evidence, red line, limits). Restate that `float_tolerance` stays the absent-record bound (lines ~185) and add the `laya-verify` advisory role text (D-10). Validate with `./target/release/pv validate|lint contracts/laya-parity-v1.yaml` (file form; directory form fails at Gate 10 in this tree) and `pv diff` of `git show HEAD:... > old.yaml` vs new for the version bump.

### `crates/aprender-decide/src/verify.rs` (verifier)

**Analog:** itself.

**View struct to extend** (lines 474-480; no `deny_unknown_fields`, so added keys are rollback-safe):
```rust
pub struct ParityConstantsView {
    pub pack_rescore_noise_k: f64,
    pub pack_rescore_bound_max_abs: f64,
}
```
Add the six per-set fields; map them in `from_contract_views` (line 499, THE one mapping; `pack_laya` and tests use it) into new `VerifyPolicy` fields (struct at line 98). Keep `rescore_probs_abs`/`rescore_noise_k` as the stamp values.

**Core to change** (`rescore_bounds`, lines 2394-2450): header and reported-`bound` cross-check keep the STAMP formula; only enforcement moves per set:
```rust
let bound = floor.max(policy.rescore_noise_k * noise);
if set.bound.to_bits() != bound.to_bits() { return Err(VerifyError::RescoreNoiseMismatch { which, field: "bound", reported: set.bound, recomputed: bound }); }
if !within(bound, policy.rescore_bound_max_abs) { return Err(VerifyError::RescoreBoundCeiling { which, bound, ceiling: policy.rescore_bound_max_abs }); }
```
Change: keep the first two as `stamp_bound`; compute `enforced = per_set.floor.max(per_set.k * noise)`; ceiling check against the per-set ceiling; store `enforced` in `RescoreBound`. Absent-record path (`bound: floor`) stays at `float_tolerance` for both sets (protects `fail_closed_vectors`). Add a load-time invariant: `floor_s >= float_tolerance`, `ceiling_s >= floor_s`, finite. No `unwrap()`; return typed errors.

### `crates/aprender-decide/src/verify/tests.rs` (test)

**Analog:** `write_noise_record` (1664-1700). Key line to keep on the STAMP fields (1677):
```rust
let bound = policy.rescore_probs_abs.max(policy.rescore_noise_k * noise);
```
and header `("k", policy.rescore_noise_k.into())` (1703). Add per-set cases where the stamp bound and the per-set enforced bound differ (so a swapped field turns a test red), a v1-record-verifies-under-amended-policy case, a wrong-k record still refused, a per-set ceiling refusal, and run the mutation check (CLAUDE.md Verification Discipline 4, 7). Model new tests on `noise_absent_uses_floor` (1751) and the existing `noise_k_mismatch` / `forged_noise_value` tests.

### `justfile` `laya-deploy` step (5) and `laya-upload` (recipe edits, D-08..D-10)

**Analog:** current blocks. `laya-deploy` 2516-2533:
```bash
    mkdir -p "$SD"
    VLOG="$SD/eligibility-$SERVER.log"
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    set -e
    if [ "$vrc" -ne 0 ]; then
        refuse eligibility "$(grep -m 1 '^REFUSED' "$VLOG" || echo "laya-verify exited $vrc (log: $VLOG)")" "$vrc"
    fi
    python3 - "$VLOG" "$H" <<'PY' || refuse eligibility "..."
    import json, sys
    lines = [l for l in open(sys.argv[1]) if l.startswith("{")]
    v = json.loads(lines[-1])
    sys.exit(0 if v.get("deploy_eligible") is True and v.get("artifact_sha256") == sys.argv[2] else 1)
    PY
```
`laya-upload` (4082-4100) is the same block with `VLOG="$SD/eligibility-upload-$SERVER.log"` and `H_LOCAL="$(sha256 "$APR")"`. Edit both identically (shared snippet; consider a `_laya-pack-record-check` helper recipe like `_laya-account-check`/`_laya-target-check`). Logic (RESEARCH Pattern 3): record path `${LAYA_PACK_OUTCOME:-$(dirname "{{run}}")/logs/pack-outcome.json}`; if present require `schema == "laya-train-pack-outcome-v1"`, `accepted is True`, `deploy_eligible is True`, `apr_sha256 == $H`; run `laya-verify`; only a first line starting `REFUSED RescoreDrift` with the record check passed is downgraded to a logged cross-CPU finding in `$SD`; any other refusal still blocks; no record keeps today's behaviour. Use the `refuse <class> "<msg>" [rc]` helper and `python3 -I - ... <<'PY'` heredoc style already in the recipe. Lint with `bashrs`.

### `justfile` `laya-deploy-selftest` + `scripts/laya_gates.tsv` (gate table)

**Analog:** tsv header `gate_id	target	must_fail	must_pass	finding`; existing rows `deploy-refusals`, `upload-eligibility`, `verify`, `deploy-selftest-skip` expect `REFUSED eligibility SyntheticNotDeployable` for the fixture. Add cases (record accepted + sha match + RescoreDrift => finding + continue; sha mismatch; accepted false; non-RescoreDrift refusal still blocks; no record unchanged) and one tsv row per new gate with must_fail and must_pass columns filled.

### `15-REPRODUCTION-DECLARATION.json` (D-11, D-12, D-14)

**Analog:** `14-REPRODUCTION-DECLARATION.json`, schema `laya-reproduction-declaration-v1`. Copy keys: `must {deploy_eligible, device_prefix:"cuda", recipe_id, inputs_sha256{task_json,train_jsonl,eval_jsonl}, seeds}`, `must_note` (reword for D-08), `band {margin:[0.30,0.45], ece_post:[0.07,0.12]}` unchanged, `report_only`, `owner_declaration_reply`. Set `retry.attempt_cap: 1`, `retry_classes: []`, `waiver_rule: "inconclusive"`. Changed hashes/recipe_id for any D-14 change. Validate with `python3 -I scripts/laya_deploy/laya_repro_eval.py check-declaration`. Commit before the paid run (committed-at timestamp vs training start proves ordering). Note RESEARCH Pitfall 4: lr, batch size, warmup, seed set are pinned by `laya-finetune-gate-v1.yaml`; only epochs [4,12] and data are free.

### `15-GATE-RUN-EVIDENCE.json` and owner-decision record

**Analog:** `14-GATE-RUN-EVIDENCE.json` top-level keys: `schema, attempts, verdict, verdict_reason, claim_scope, next_trusted, declaration_sha256, declaration_commit, owner_replies, pack, noise, ...`; produced by `laya_repro_eval.py evaluate --declaration DECL --run-dir DIR --attempt 1`. Claim scope must say the run is not an identical reproduction (D-14). Owner replies: copy the `14-GAP-ROUND-2-OWNER-DECISIONS.md` form (header "Recorded by the orchestrator", table of question and verbatim answer, "Consequences" list); recorded on disk before the contract edit (git order is the SC1 evidence).

## Shared Patterns

### Record-first, one-way action gating
**Source:** `14-GAP-ROUND-2-OWNER-DECISIONS.md`, `14-REPRODUCTION-DECLARATION.json`
**Apply to:** the contract edit (needs the recorded bound sign-off), the re-pack and the paid run (each needs an owner-recorded approval; AWS commands are owner-run with the bang prefix; subagents do not relay approvals).

### One implementation of the bound
**Source:** `verify.rs::VerifyPolicy::from_contract_views` (499) and `rescore_bounds`
**Apply to:** all plans. No second copy in Python or in a recipe; Python only stamps the record (`scripts/laya_train/contract.py::noise_policy`, untouched under Option S-B).

### Mutation-proved refusals
**Source:** `verify/tests.rs` noise tests, CLAUDE.md Verification Discipline 4 and 7
**Apply to:** every new refusal/accept rule in Rust and in the recipe case table.

### Evidence hygiene
**Source:** `scripts/laya_deploy/laya_drift.py` (scrubs 12-digit account ids)
**Apply to:** all committed evidence: no account ids/ARNs; cite pack image digest and `parity_contract_sha256` (equals sha256 of the committed amended contract) as proof the new bound engaged.

### Image and Lambda roll order
**Source:** RESEARCH Pitfalls 2, 3
**Apply to:** commit decide edits, owner-run `just laya-trainer-image pack`, then `just laya-trainer-synth dev`, then `infra diff` and `infra deploy`, no further commits to the cargo closure between synth and deploy.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `_laya-pack-record-check` helper (if created) | recipe helper | request-response | No existing recipe reads `pack-outcome.json`; the shape is in RESEARCH "Recipe gate"; copy helper style from `_laya-account-check` |
| Cross-CPU finding log line | utility | event-driven | New; write to `$SD/cross-cpu-finding-$SERVER.log` |

## Metadata

**Analog search scope:** `crates/aprender-decide`, `contracts/`, `justfile`, `scripts/laya_gates.tsv`, `scripts/laya_deploy/`, Phase 14 directory
**Files scanned:** ~12 read in part
**Pattern extraction date:** 2026-10-31
