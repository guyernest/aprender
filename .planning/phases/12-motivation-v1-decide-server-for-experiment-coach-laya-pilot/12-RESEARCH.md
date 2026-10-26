# Phase 12: motivation-v1 decide server for experiment-coach (pilot, augury-dev) - Research

**Researched:** 2026-10-26
**Domain:** Laya fine-tune back office (Python) + Rust pack/verify gate + thin pmcp.run decide server; SetFit and Kev-4B reference arms
**Confidence:** HIGH for the Laya/back-office/pack/deploy mechanics (read from source this session); MEDIUM for SetFit arm feasibility; LOW for the Kev arm (several blockers, never run at 4B in Rust)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Owner decisions already made (2026-10-26, from ROADMAP.md, recorded as given)
- **D-01:** Build and deploy a pilot now, labelled as one. The guide is the 25 October draft and is not frozen;
  expect a retrain after the freeze.
- **D-02:** The deployment-unit register (FALSIFY-MONO-011, `crates/aprender-core/tests/monorepo_invariants.rs`)
  may grow by one for the new thin crate `aprender-mcp-decide-motivation-lambda`. This is the recorded owner
  decision Phase 10's learnings said growth requires; the duplicate-bin allowlist grows with it.
  — **Reversibility:** costly — the register is shrink-only; undoing means a retirement like D-20 of Phase 10.
- **D-03:** No server-side prior correction. Serving stays `softmax(z / T)`; serve calibrated probabilities and
  document the training prior (other ≈ 64%, against ≈ 84% in the guide's Table 7 corpus) so experiment-coach can
  reweight. Server-side correction is deferred to the post-freeze retrain.
- **D-04:** Results by source batch are NOT measured (the mapping is not available). Results by length are.

#### Bake-off rules
- **D-05:** Ranking metric is macro-F1 on the shared eval split. The false-assignment rate (eval `other` posts
  whose top non-`other` probability exceeds the threshold) is REPORTED for every arm at several thresholds,
  including experiment-coach's current `motivation_min_prob` 0.6 and the guide's example 0.85, and is used only as
  a tiebreak. No hard false-assignment ceiling in the pilot; one may be declared for the post-freeze retrain.
- **D-06:** Laya is the served arm regardless of the ranking. SetFit and Kev are reference arms. If a reference arm
  clearly beats Laya, that is recorded as a finding in the hand-off, and switching methods is a later phase.
- **D-07:** The Kev arm is Kev-4B run through the Rust path (PEFT merge → GGUF → `Qwen35Model` + pointer head,
  with batched prefill) from the unpushed branch `spike/016-upstream-sync` in the existing worktree
  `../aprender-016-upstream-sync`, USED IN PLACE. The branch is not landed. The evidence records the branch name and
  commit SHA. It runs locally only (about 16.8 GB in f32; it does not fit Lambda).
- **D-08:** The SetFit arm reuses the Phase 5 / Phase 10 cell recipe on the same train and eval splits.
  `apr eval`'s memory growth blocked Phase 10's cell at 2,129 rows. At about 99 eval rows it is expected to fit
  (measured 9.4 GB at 61 rows and 40.3 GB at 241), but this must be confirmed by an s8 rehearsal before the cell
  runs (Phase 10 lesson).

#### Guide → task.json
- **D-09:** The guide's Section 9 model instruction (definitions, indicator tables, decision rules, 24 worked
  examples: about 4,500 words for Sections 4–6) cannot fit Laya's `head_max_len` 192. aprender drafts TWO
  condensed variants from Sections 4–6, as in Phase 10:
  (a) one that keeps the served door at 8 texts / 800 built tokens (P ≤ 100), and
  (b) a fuller one up to the 192-token head with a correspondingly lower `classify_max_texts` for this server.
  Both are measured with the served tokenizer (`--measure`), and the measured P and admitted door go into a table.
  The owner picks at a checkpoint BEFORE the gate run. If (b) is chosen, the plan must say whether the lower door is
  per-server configuration or a `decide-tool-boundary-v1` change.
  — **Reversibility:** one-way once the gate runs — the task.json bytes are part of the artifact identity
  (`inputs_sha256.task_json`).
- **D-10:** The owner approves the exact task.json bytes at a blocking checkpoint before the gate run. Its sha256
  is recorded together with the guide docx's sha256 and its "25 October 2026 draft" designation. Routing the
  draft to Dr. Yuster is not required for the pilot.
- **D-11:** The Kev arm's prompt carries the full guide Sections 4–6 text. Its few-shot examples come from TRAIN
  split rows only. The guide's Section 10.3 example set is exactly the dataset's 89 S + 55 C posts, and its Table 6
  worked examples may also be dataset rows, so any worked example whose text appears in the eval split is removed
  from Kev's prompt. The leakage check is by text match and is recorded.
- **D-12:** The served output is the existing `classify` contract only (calibrated `probabilities` aligned to
  `labels`, plus `tokens` and `truncated`). The guide's Section 9 audit output (evidence spans, evidence levels,
  doubt flag, irony flag) is deferred.

#### Split and metrics on 395 rows
- **D-13:** One seeded, stratified, text-disjoint 25% hold-out `eval.jsonl` (about 99 rows: about 14 C, 22 S and
  63 other), shared by the Laya gate and all three arms. Laya runs three seeds, 13/17/23, with the median-ECE seed
  shipped (unchanged policy). ECE is reported with a bootstrap confidence interval so its noise on about 14 C rows
  is visible. If `laya-finetune-gate-v1` or the back office fixes the eval fraction at 20%, the 25% fraction is
  declared as a contract amendment before the run.
- **D-14:** Length bands are character-length tertiles of the eval set, with the cut points recorded. Per-class
  precision and recall are reported within each band for every arm. This tests the length shortcut directly (the
  medians are 243 S / 198 C / 152 other).
- **D-15:** The hand-off states the source-batch shortcut as UNMEASURED, a known blind spot. The owner holds the
  mapping for the retrain.
- **D-16:** The prepare script follows the `prepare_goldstandard.py` pattern: rules declared in the docstring
  before the run, the source CSV's sha256 and per-class counts pinned, idempotent output, and counts and sha256s
  printed but never post text. The label map is `Social-affiliative` → `social_affiliative`,
  `Conspiratorial explanation` → `conspiratorial`, `None` → `other`. The data dir lands under the gitignored
  `/data/` root.

#### Calibration and gate failure
- **D-17:** The calibration-slice size is set by the FOLDED spike (see Folded Todos), measured on this data before
  the gate run is declared, and declared in the gate contract.
- **D-18:** If the spike shows the `calibration_temp_max` 5.0 clamp is binding on this data, it STOPS for the owner
  as an architectural decision. Raising the cap changes `laya-parity-v1` and the Rust clamp, and diverges from
  upstream Laya. It is not pre-authorised.
- **D-19:** If the gate run passes margin but FAILS ECE (≤ 0.10), the pilot SHIPS anyway, labelled a gate-failed
  pilot. Mechanism: a `pilot_waiver` clause declared in the gate contract BEFORE the run (in
  `laya-finetune-gate-v1` or a motivation-specific gate contract; the planner chooses which). It names exactly the
  ECE clause it waives and the artifact it applies to. `aprender_decide::pack` honours only a declared waiver, the
  artifact metadata carries the waived status so the pinned sha shows it, and pack still refuses any undeclared
  failure. The hand-off states the failure and the measured ECE with its CI.
  — **Reversibility:** costly — a waiver path in the packer is a permanent capability of a release-gating tool; it
  needs a falsification test proving an undeclared failure is still refused.
- **D-20:** A MARGIN failure (Laya not beating zero-shot macro-F1 by ≥ 0.05) HALTS for the owner. It is not
  covered by D-19. Options then: one budgeted re-run, asking Dr. Yuster for more clear Conspiratorial posts and the
  weak-evidence group, or stopping.
- **D-21:** No training-time class reweighting. Train on the file's prior; the recipe is unchanged.
- **D-22:** The hand-off documents client-side prior correction for experiment-coach: the formula
  `p'_k ∝ p_k · π_field,k / π_train,k`, the train-split class counts as π_train, the guide's Table 7 figures as the
  field reference (with their caveat that weak and mixed evidence count as `other`), and a statement that no server
  change was made.

#### Carried forward from Phase 10 (unchanged)
- Recipe and gate thresholds: margin ≥ 0.05 over zero-shot, top-label ECE ≤ 0.10, early stopping, epochs declared
  from a timed one-epoch probe by a rule written before the probe, and the device read back from a trace line.
- One declared gate run, no retuning after reading results.
- Each served model gets its own server id and thin `<server>-lambda` crate on the shared
  `aprender-mcp-decide-lambda` library; the server id is the argument to the AWS-touching just recipes.
- Deploy goes through `just laya-build-bootstrap` / `laya-deploy-config` / cargo-pmcp to pmcp.run, 10,240 MB tier,
  auth off. The live deploy is a human checkpoint; the owner logs in to pmcp.run right before the step that needs
  the token (the session lasts about 1 h).
- F-10-12-1: a brand-new server id's first edge `/health` can 404 and trigger containment. The plan must handle
  this (bounded retry before containing, or an owner-run step). F-10-12-2: a sticky 503 error state was cleared by
  redeploying.
- The acceptance replay tests function and identity: one result per text, labels in task order, probabilities
  summing to 1, and the returned sha equal to the pin. Cold and warm timings are reported, not gated. A longer
  prefix eats into the 4.5 s cold-start slack to the 30 s cap, so record it.
- Never assert `tokens_planned == texts × P`.

### Claude's Discretion
- The split seed and grouping rule, declared in the prepare script.
- The wording of the two condensed task.json drafts; the owner approves the final bytes (D-10).
- Whether the waiver lives in `laya-finetune-gate-v1` or a new motivation gate contract, and the exact metadata
  field name.
- How the Kev arm reads the shared split (an adapter from the Laya data dir, under OPS-03) and how its pointer head
  is fitted for three labels from train rows.
- The bootstrap CI method for ECE and the number of resamples.
- Evidence file names (`12-*-EVIDENCE.json`, following Phases 8 and 10).

### Folded Todos
- **`spike-laya-calibration-slice-and-temperature-cap.md`** (from Phase 8 plan 08-08). The original problem: the
  stance demo failed ECE twice. Causes were the T ≤ 5.0 cap (the eval-optimal T was about 7.5) and a 12-row
  calibration slice whose stopping-signal standard error (0.22) swamped the differences between epochs. How it fits
  here: motivation-v1 has the same thin-data shape (about 41 C rows in train before the slice is drawn). The spike
  runs on THIS dataset BEFORE the gate is declared. It measures fitted T against eval-optimal T across slice sizes
  and seeds 13/17/23, and reports whether the T ≤ 5 clamp binds. Its slice recommendation is declared in the gate
  contract (D-17); a binding clamp stops for the owner (D-18). It is a measurement, not a gate run. The todo's
  second part (whether the 1e-5 re-score bar sits at fp32 noise) is already resolved by amendment A1 (plan 08-13).
  The plan re-measures the re-score tail on the shipped checkpoint, as pack does, and nothing more.

### Deferred Ideas (OUT OF SCOPE)
- Server-side prior correction: the post-freeze retrain (D-03).
- Per-source-batch results: when the owner supplies the mapping (D-04, D-15).
- Evidence-span and audit output from guide Section 9 (D-12): a new tool or field, its own phase.
- A hard false-assignment ceiling as a gate clause: the post-freeze retrain (D-05).
- Serving SetFit or Kev if either beats Laya: a later phase (D-06).
- Landing `spike/016-upstream-sync` (upstream merge, MTP `nextn_predict_layers` fix, batched prefill): separate work (D-07).
- Thread context (parent plus up to three preceding posts), per guide Section 11: the development data has none and `classify` sends the target only.
- Accuracy on posts not viewed during development, as the guide requires: when the partner supplies them.
- Raising `calibration_temp_max` beyond 5.0: only if the folded spike shows it binding, and then as an owner architectural decision (D-18).

#### Reviewed Todos (not folded)
- `aprender-train-gpu-ledger-tests-red.md`, `qwen35-9b-hybrid-forward-unimplemented.md`, `workspace-test-gate-blocked-by-renacer-validate.md`: keyword matches only, unrelated to this phase.
</user_constraints>

<phase_requirements>
## Phase Requirements

No requirement IDs are mapped (`null`). Must-haves are derived from CONTEXT D-01..D-22 and the ROADMAP goal
(`.planning/ROADMAP.md:943-959`). The table below maps each decision cluster to the research that enables it.

| ID | Description | Research Support |
|----|-------------|------------------|
| D-13/D-16 | Prepare script, 25% stratified hold-out, pins | §Q1 (eval fraction is script code, not contract); dataset facts measured below |
| D-09/D-10 | Two task.json variants, measured door, owner approval | §Q4 (prefix room measured; (b) needs a `decide-tool-boundary-v1` change) |
| D-17/D-18 | Folded calibration spike, clamp-binding stop | §Q3 (train-only holdout design, reuse spike-027 harness; global-constant hazard) |
| D-19/D-20 | Declared pilot waiver; margin halts | §Q2 (exact refusal site, design A, falsification tests) |
| D-05/D-14/D-22 | Bake-off metrics, false-assignment, tertiles, ECE CI, prior | §Q9 (one metrics module; new functions; SetFit needs per-row probs) |
| D-07/D-11 | Kev-4B Rust arm | §Q5 (blocking disk, no 4B GGUF, guide length vs Kev branch limit, cost) |
| D-08 | SetFit arm | §Q6 (adapter is 2-label only; 64 shots infeasible; `--dump-probabilities` needed) |
| D-02 | Thin crate + register 9→10 + allowlist | §Q7 (every file that must change) |
| F-10-12-1/2 | New-id `/health` 404 | §Q7 (no retry exists today: `justfile:2068`) |
| Carried | Cold start, acceptance replay, hand-off | §Q8, §Q10 |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Directives the planner must honour (from `/Users/guy/Development/machine-learning/aprender/CLAUDE.md` and the user's global CLAUDE.md):

- `main` is protected; work on a branch, PR, CI (`ci / gate` + `workspace-test`) must pass. Modifying `.github/workflows/*.yml` is a check-in item.
- Use `justfile` recipes for scripts (user global). Existing `laya-*` recipes are the canonical entry points.
- **`unsafe_code = "forbid"`, `unwrap()` banned** (`.clippy.toml` disallowed-methods); use `expect()` / `ok_or_else(..)?`.
- **Contracts are validated with `pv`, never a bash/yq/python re-implementation.** `pv` is NOT on PATH on this host; a built `target/release/pv` exists (10.4 MB) — rebuild with `cargo build --release -p aprender-contracts-cli` before use so it is current. `pv diff` takes two FILE paths (materialize the old revision with `git show`).
- **Coverage + contracts co-evolve**: new Rust functions (the waiver path) need a `#[contract]` annotation and falsification tests in the contract YAML (Rule 7).
- **Pin the `apr` binary** with `. scripts/apr_bin.sh || exit 1` → `"$APR"`; never a bare `apr` (the SetFit arm uses `apr data select`, `apr setfit train`, `apr eval`).
- Verification discipline: never read `$?` through a pipe; prove a mechanism engaged (device from the `DEVICE ... used=` trace line); vary a failing input before naming a cause; a guard needs a case table; extending a guard's scope requires re-mutating it there.
- Shell: `bashrs lint`; `set -euo pipefail`; a sourced library must be option-neutral.
- Code search policy: `pmat query` over grep (this research used `pmat query` for discovery and `Read` for every cited value).
- Realizar-first architecture: the Decision-model row is a documented exception (Phase 8 D-16); the motivation server stays transport-only over `aprender-decide` (no numerics in the thin crate).
- README crate count and contract count are EQUALITIES checked by `crates/aprender-core/tests/readme_contract.rs` (`test_readme_crate_count_matches_workspace`, `test_readme_contract_count_matches_workspace`); every crate needs a README linking the monorepo (`test_every_crate_has_readme`, `test_every_readme_links_monorepo`); every repo path cited in CLAUDE.md must exist (`test_documented_paths_exist`).
- Autonomous mode: ship PRs; but compute spend > 1 h on non-lambda-vector hosts, destructive ops, and architectural pivots are check-ins. The Kev arm (multi-hour local compute) and any T-cap raise are check-ins.

## Summary

Phase 12 repeats Phase 10's pipeline on a 395-row, 3-label dataset, and almost all of the machinery already supports it: the back office (`scripts/laya_train`) is K-generic and was proven at 3 labels by the Phase 8 stance demo (criteria `[none, against, favor]`, `laya-finetune-gate-v1.yaml:302`); the 3-label temperature bucket is the stance demo's `choice:3-5` (`temperature.rs:16`); the 25% eval fraction is a prepare-script constant, not a contract value, so D-13's "amendment" branch does not trigger. The new code is: a `prepare_motivation.py` (copy of `prepare_goldstandard.py`), the D-19 waiver in the Rust verifier, a bake-off metrics script, the SetFit adapter generalised to 3 labels plus the deferred `apr eval --dump-probabilities`, one thin crate, and a bounded `/health` retry in `laya-deploy`.

Five findings change the plan's shape. (1) **Variant (b) of D-09 cannot be per-server configuration today**: `build_server` serves only `ClassifyLimits::CONTRACTED` and refuses at build time when 8 × P > 800 (`aprender-mcp-decide/src/lib.rs:751-792`), so any P > 100 needs a `decide-tool-boundary-v1` change. (2) **The calibration-slice fraction is ONE global constant read by both Python and the Rust verifier, which enforces a minimum** (`verify.rs:1709-1734`); raising it globally would make the deployed antisemitism-v1 artifact fail re-verification, which `laya-deploy`/`laya-upload` re-run before every deploy. Keep 0.25 unless the spike shows clear benefit, and if it does, add a per-run override rather than editing the constant. (3) **The folded spike must not read `eval.jsonl` labels**: "eval-optimal T" on the gate's own eval split would contaminate the one declared gate run; run the spike on a hold-out carved from TRAIN. (4) **The SetFit arm needs changes before it can run**: the adapter refuses anything but 2 labels (`laya_dir_to_canonical.rs:161-166`), 64 shots/class is impossible (C pool ≈ 36 after the validation carve; shots ∈ {8,16,32,64}, `select.rs:59`), and macro-F1 needs per-row probabilities that only the deferred plan 10-06 (`--dump-probabilities`) provides. (5) **The Kev arm is blocked as specified**: 25 GiB free disk against ≈ 34 GB transient for a 4B fp32 merge + GGUF; no Kev-4B GGUF exists; the Rust path was only ever run at 0.8B; Sections 4–6 including §4.5 are 9,648 Qwen tokens, above Kev's 8,192-token serve row limit, and even the §4.5-free 6,811-token text is 6.6× Kev's 1,024-token training branch, which Kev itself marks "untested".

**Primary recommendation:** Plan Laya end to end first (prepare → task variants + owner checkpoint → train-only spike → probe → declare slice + waiver + epochs → one gate run → pack/verify → thin crate → deploy → replay → hand-off); run SetFit after its adapter and dump changes; put the Kev arm behind an owner feasibility checkpoint (disk + time + guide-length decision) with a declared cost-probe, and let the hand-off record it as blocked if the owner says so.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Data prep, split, task measurement | Back office (Python `scripts/laya_train`) | — | Training stays in Python (llm-decision-classifier requirement); never prints text |
| Fine-tune, calibration fit, gate report | Back office (Python `train.py`) | — | Writes the run dir; decides nothing deployable |
| Gate re-derivation, waiver honouring, pack | Rust back office (`aprender-decide::verify` / `pack_laya`) | Contracts (`laya-finetune-gate-v1`) | The ONLY eligibility door (`pack_for_serving`, `verify_path`); contract is the policy source |
| Artifact identity (waived status in sha) | `decide-apr-v1` artifact manifest | Gate-report blob | Everything in the manifest and blobs is inside the pinned sha |
| Request bounds (door) | `aprender-mcp-decide` (compiled from `decide-tool-boundary-v1`) | — | Build-time `check_served_task_fits` + per-call bounds |
| Lambda bootstrap | Thin crate `aprender-mcp-decide-motivation-lambda` | `aprender-mcp-decide-lambda` library | Shim only (`tests::thin_server_crates_are_shims`) |
| Deploy, edge checks, containment | `justfile` `laya-*` recipes + cargo-pmcp | pmcp.run / AWS | Server id is a required argument |
| Bake-off metrics | Back office (`scripts/laya_train/metrics.py` + new report script) | — | One implementation per operation (OPS-03) |
| SetFit reference arm | `apr` CLI (`apr data select` / `setfit train` / `eval`) | `aprender-contrastive-data` adapter | Existing pinned path |
| Kev reference arm | Python kev (tokenize, bias fit) + Rust `Qwen35Model::prefill_hidden` (worktree) | — | D-07: Rust forward from the unpushed branch |
| Prior correction | Client (experiment-coach) | Hand-off document | D-03/D-22: no server change |

## Standard Stack

No new external packages are required. Everything below is already in the repo or its locked environments.

### Core
| Component | Version / Pin | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `scripts/laya_train` (uv project, `uv.lock`) | torch 2.14.0 (Phase 10 gate run), Laya @ `4066d5d` | Fine-tune + calibrate + gate | The declared back office [VERIFIED: laya-finetune-gate-v1.yaml:285 `laya_git: 4066d5d5fbf08b66c6757ddeedbd797bd7655bc0`] |
| Laya-en base snapshot | revision `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`, model sha `891102d3…` | Declared base | [VERIFIED: laya-finetune-gate-v1.yaml:282-284] present in `~/.cache/huggingface/hub/models--convaiinnovations--laya/snapshots/55cf4c4e…/` |
| `aprender-decide` (`pack_laya` example) | in-tree | pack / verify / inspect | The only eligibility door |
| `aprender-mcp-decide-lambda` | in-tree library (`bootstrap::run`) | Shared Lambda runtime | Phase 10 D-19 pattern |
| cargo-pmcp | 0.28.1 installed | pmcp.run deploy | Resolver proof is per version |
| cargo-zigbuild | 0.23.4 installed | arm64 bootstrap build | `laya-build-bootstrap` requires it |

### Supporting
| Component | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `apr` (pinned via `scripts/apr_bin.sh`) | HEAD build | SetFit arm | `apr data select`, `apr setfit train`, `apr eval` |
| MiniLM checkout `~/.cache/aprender/minilm-l6-v2-1110a243/` | `full_model.apr` 86.7 MB present | SetFit encoder | `laya-setfit-cell` requires `full_model.apr` |
| Kev vendor `.planning/spikes/015-kev-vs-setfit-few-shot/vendor/kev` | `7405b72e73e2d24787f3720d162a21c974ff2ad2`, `.venv` present | Kev tokenizer / row encoder / bias fit | Kev arm only |
| llama.cpp vendor `.planning/spikes/017-kev-rust-forward-parity/vendor/llama.cpp` | `convert_hf_to_gguf.py` present | HF → GGUF | Kev arm only |
| Worktree `../aprender-016-upstream-sync` | branch `spike/016-upstream-sync` @ `32103ba83ce4165598ae31ddde89b5c9856b43f3` | `Qwen35Model::prefill_hidden` | Kev arm only (D-07) |
| `target/release/pv` | in-tree build | contract validate / diff | every contract edit |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Waiver in `laya-finetune-gate-v1` | New `motivation-gate-v1` contract | A second contract needs a second policy reader (`pack_laya.rs:72-76` reads exactly two contracts) and bumps the README contract-count equality. In-place is smaller. |
| SetFit s64 cell | s32 cell | 64 is infeasible at this data size (see §Q6); s32 is the largest allowed shot count that fits |
| Kev-4B Rust f32 | Kev-4B Python/MLX bf16 | Much faster and needs no GGUF, but contradicts locked D-07; only with an owner amendment |

**Installation:** none. **Version verification:** no registry packages are added; the Package Legitimacy Gate is not triggered.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| (none) | — | — | — | — | — | No external package is installed by this phase |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

## Dataset and guide facts (measured this session, counts only)

All measured from the files with a counts-only script (no post text printed). [VERIFIED: shasum + python over the CSV and the docx, 2026-10-26]

| Fact | Value |
|------|-------|
| CSV | `datasets/motivation/Motivation Dataset - Sheet1.csv`, sha256 `bc61c76bd29bcdd4c6466266b4e1161841f2107a4f02f5ff4fd96f1b7a3a4ae7` |
| Header | `["Post (English, original)", "Class"]`, every row 2 fields |
| Rows / distinct under NFC+trim+ws-collapse | 395 / 395 (no duplicate groups; empty texts 0) |
| Class counts | `Social-affiliative` 89, `Conspiratorial explanation` 55, `None` 251 |
| Guide docx | `Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx`, sha256 `0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167` |
| Partner repo HEAD | `0c46b68887c4b0791693462ac845df2a2da8511d` (the request commit) |
| 25% stratified eval, `ceil` per class | C 14, S 23, other 63 = **100** rows (CONTEXT's "about 99" — exact is 100) |
| Train | C 41, S 66, other 188 = 295 |
| Calibration slice at 0.25 (`max(2, ceil(0.25·n))`) | C 11, S 17, other 47 = 75; fit rows C 30, S 49, other 141 = 220 |
| Post tokens (served Laya tokenizer) | p50 45, p75 61, p90 69, p95 77, p99 108, max 147 |
| Post tokens by class (median) | C 47, other 35 |
| Character length | medians S 243 / C 198 / other 152 (matches the request); whole-file tertiles 122.3 / 243.0; max 428 |
| Guide §4–6 (lines incl. §4.5) | 6,293 words; 9,478 Laya tokens; **9,648 Qwen (Kev-4B) tokens** |
| Guide §4.1–4.4 + §5 + §6 (no §4.5 "derivation") | 3,969 + 274 + 247 = 4,490 words (= CONTEXT's "about 4,500"); 6,114 + 374 + 323 = **6,811 Qwen tokens** |
| Guide §7 Table 6 worked examples | schematic, constructed with `[CLAIM]`/`[TAG]` placeholders; guide line: "Authentic examples from the Jikeli et al. (2023) dataset are kept in the internal example set (Section 10), not in this document" |
| Dataset rows quoted in §4–7 (first-60-char normalized match) | 0 of 395 |
| Field prior (guide §10.2 Table 7) | of 1,546 classified: no evidence 710, weak only 585 (≈ 84% Other); "about 7 percent … clear social-affiliative … about 9 percent clear conspiratorial" |
| Dataset source | guide §10.1: "The Gold Standard 2024 dataset (Jikeli et al., 2023) … 1,953 are labeled antisemitic" — the same corpus antisemitism-v1 trained on |

Implication for D-11: the leakage check against the guide text is trivially clean (0 matches; Table 6 is schematic). The only leakage risk is Kev few-shot rows, which must come from `train.jsonl` only.

## Research Question Answers

### Q1. Back office: eval fraction, calibration slice, 3 labels, zero-shot macro-F1

**Eval fraction is script code, not a contract value.** [VERIFIED: scripts/laya_train/prepare_goldstandard.py:96-99, 166-168]
```
LABELS = ["not_antisemitic", "antisemitic"]            # D-02: criteria document order is the label index
LABEL_OF = {"0": 0, "1": 1}
SPLIT_SEED = 13                                         # RULE split: the contract's declared seed
EVAL_DIVISOR = 5                                        # RULE split: 0.2 = 1 / 5, ceil done in integers
...
def eval_count(n_groups):
    """ceil(0.2 x n_groups), exactly, in integers."""
    return (n_groups + EVAL_DIVISOR - 1) // EVAL_DIVISOR
```
`laya-finetune-gate-v1` has no eval-fraction key: `eval_set.generic_rule` only requires held-out data "drawn like train.jsonl" and every criterion present [VERIFIED: laya-finetune-gate-v1.yaml:192]. The Rust verifier checks eval coverage, not size (`check_eval_coverage`, `verify.rs:1298`). **So D-13's amendment branch does not trigger**: `prepare_motivation.py` declares `EVAL_DIVISOR = 4` (25%) in its docstring before the run.

**Calibration slice: one global contract constant, applied in Python and enforced as a MINIMUM in Rust.** [VERIFIED: laya-finetune-gate-v1.yaml:155-161]
```
  calibration_temp_min: 0.5
  calibration_temp_max: 5.0
  calibration_slice_fraction: 0.25
  calibration_slice_min_per_class: 2
  # ---- seeds (D-08) ----
  declared_seed: 13
```
Python draws it in `data.calibration_split`, `need = max(int(min_per_class), int(math.ceil(float(fraction) * counts[c])))` [VERIFIED: data.py:283-306], called from `train.py:751-754` with the contract constants and `seed` = the declared seed 13 (the slice does NOT vary with the gate seed; `train.py:53-54`: "The seed varies the training RNG … not the data or the calibration slice"). Rust re-derives `slice_need` and refuses `if (in_slice as u64) < need || in_slice >= in_class` [VERIFIED: crates/aprender-decide/src/verify.rs:1709-1734]. The fraction is not a CLI flag (`train.py:708-723` has `--data --out --epochs --stopping --seeds --device --variant --base --base-sha256` only).

**3 labels end to end: supported and proven.** `train.py` takes `k = len(task["labels"])` everywhere; the stance demo that passed the gate and was served had 3 criteria [VERIFIED: laya-finetune-gate-v1.yaml:302 `criteria_order: [none, against, favor]`, :357 `outcome: gate_pass`]. The 3-label calibration bucket is the stance demo's: [VERIFIED: crates/aprender-decide/src/laya/temperature.rs:16 `/// Laya's option-count bucket: "{qtype}:2", ":3-5", ":6-10" or ":11+".`].

**Zero-shot macro-F1 for 3 labels:** the declared base scored on the same eval rows (`train.py:792-801`), macro-F1 = mean F1 over labels present in `y ∪ pred` [CITED: scripts/laya_train/metrics.py docstring lines 5-7; laya-finetune-gate-v1.yaml:520]. `f_avg` is null for this task (`train.py:789-790` sets it only when labels equal the stance demo's criteria order).

**Margin pitfall specific to this phase:** the zero-shot baseline uses the SAME task.json. A fuller, better instruction (variant b) can raise zero-shot macro-F1 and so shrink the margin that D-20 halts on. Report zero-shot macro-F1 per variant only after the gate run (it reads eval labels); do not measure it before the owner's choice.

### Q2. Gate re-derivation in Rust and the minimal D-19 waiver

**Where pack refuses a gate-failed run.** `pack_for_serving` → `cheap_checks` → `artifact::write_decide_apr` → `verify_loaded`; the gate decision is: [VERIFIED: crates/aprender-decide/src/verify.rs:3266-3287]
```rust
    let clauses = check_gate(&inputs.gate_report, &recomputed, n, policy)?;
    if !clauses.is_empty() {
        return Err(VerifyError::GateFailed(Box::new(GateFailure {
            ...
        })));
    }
    // seed_policy.legacy_rule: AFTER the re-scores and the gate, so a legacy run whose gate
    // fails still reports GateFailed (the 1.x fail-closed vectors keep their refusals).
    if inputs.recipe.seed_selection.is_none() {
        return Err(VerifyError::SeedPolicyMissing);
    }
```
`check_gate` returns the failed clauses after checking that the reported `pass` equals the recomputed one [VERIFIED: verify.rs:2473-2487 — `failed.push(GateClause::Margin)` / `failed.push(GateClause::EcePost)`; `if pass != report.pass { return Err(VerifyError::PassDisagrees {..}) }`]. `GateFailed` exits 3 (`verify.rs:881-882`). `pack_for_serving` writes the `.apr` ONLY after `verify_loaded` accepts (`verify.rs:3397-3411`); `verify_path` additionally requires the file to be byte-identical to a fresh pack of its run (`verify.rs:3413-3423`, `ArtifactNotFromRun`).

**What the artifact already carries.** The manifest's `gate` block copies the trainer's verdict: [VERIFIED: crates/aprender-decide/src/artifact.rs:206-215, 1213-1218]
```rust
pub struct GateSummary {
    pub pass: bool,
    pub margin: f64,
    pub ece_post: f64,
    pub report_sha256: String,
}
...
        gate: GateSummary {
            pass: report.pass,
            margin: report.margin,
            ece_post: report.fine_tuned.ece_post,
            report_sha256: sha256_hex(&inputs.gate_report_json),
        },
```
and the whole `gate-report.json` is embedded as a sha-bound blob (`GATE_REPORT_BLOB`, `artifact.rs:1184`). So a gate-failed run's `pass: false` and its `ece_post` are already inside the pinned sha; what is missing is the *waived* status and *which declaration* waived it. No server code reads `gate.pass` (searched `aprender-mcp-decide*`), so the served path is unaffected.

**Recommended design (A): a declared waiver, copied into the gate report by the trainer, bound at load, decided by verify.**

1. **Contract** — add a top-level `pilot_waivers:` list to `laya-finetune-gate-v1` (in place; `GateContractView` is "NOT `deny_unknown_fields`", `verify.rs:205-211`). Each entry:
   - `id` (e.g. `motivation-v1-pilot-ece`), `declared_on`, `reason` (D-01/D-19 text),
   - `waives: [ece_post]` — the ONLY allowed value; a contract with `margin` in `waives` must be refused when the policy is built (D-20),
   - `applies_to: {task_json_sha256, train_jsonl_sha256, eval_jsonl_sha256, recipe_id}` — all four are known BEFORE the run: the three data hashes after prepare + owner approval, and `recipe_id` = sha256 of `recipe.json`, which `contract.recipe_json(...)` builds deterministically from the variant, recipe values, `shots_per_class` (= 188, the largest train class), the declared epochs, seed 13, the base, `early_stopping` and `seed_selection` (`contract.py:275-301`, `train.py:773-776`). The plan computes it with the same function after the epochs are declared and commits it in the contract before `laya-train` starts.
2. **Python** — `train.py` writes an OPTIONAL `pilot_waiver: {id, waives}` into `gate-report.json` exactly when a declaration's `applies_to` equals this run's `inputs_sha256` + `recipe_id`. It does NOT change `pass` or the exit code (GATE FAIL stays exit 3). Requires: a `gate_report_schema.pilot_waiver` row marked OPTIONAL, an `_OPTIONAL_KEY_RULES` entry (`contract.py:187-192`; an OPTIONAL key with no rule is refused by `expected_keys`), and a `run_field_bindings` row (the sweep fails on an unlisted leaf, `laya-finetune-gate-v1.yaml:604`).
3. **Rust read side** — `GateReport.pilot_waiver: Option<..>` with `#[serde(default)]`, exactly the 1.4.0 precedent [VERIFIED: pack.rs:351-357 `#[serde(default)] pub rescore_noise_sha256: Option<String>`, `#[serde(default)] pub shift_probe: Option<ShiftProbe>`].
4. **Rust decision** — in `verify_loaded`, when `clauses == [EcePost]` AND the report carries `pilot_waiver` AND `policy.pilot_waivers` holds that `id` with `applies_to` equal to the run's `inputs_sha256` (task/train/eval) and `recipe_id` AND `waives == [ece_post]`: return the `VerifyReport` with `deploy_eligible: true` and a new `waived: Some(id)`. Every other non-empty clause list (margin alone, margin+ECE, ECE with no/mismatched declaration) stays `GateFailed`. Map the waiver through the ONE contract→policy function `VerifyPolicy::from_contract_views` (`verify.rs:378`). Print `waived` on the `pack_laya pack`/`verify` JSON line so `laya-deploy` (which only checks `deploy_eligible` and the sha, `justfile:1987-1992`) still works and the evidence records it.
5. **Artifact identity** — add `gate.waiver: Option<String>` (the waiver id) to `GateSummary` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, filled from `report.pilot_waiver.id`, and a `manifest.bindings` row `'/gate/waiver': {rung: 4, sources: ['report:/pilot_waiver/id']}` in `decide-apr-v1` plus the ladder's leaf table (`artifact/ladder.rs:1123` region). `skip_serializing_if` is **mandatory**: `verify_path` requires an existing artifact to equal a fresh pack of its run, so if the field serialized as `null` for runs without a waiver, the deployed `laya-antisemitism-v1.apr` (sha `2293bebb…`) and `laya-stance-64.apr` would become `ArtifactNotFromRun` and every future `laya-deploy`/`laya-upload` of them would refuse.

Why not design B (verify-only, no manifest field): it satisfies "pack refuses undeclared failures", but the artifact would not name the waiver, and D-19 says "the artifact metadata carries the waived status". Design B also would leave a manifest leaf derived from policy rather than a sha-bound blob, against `decide-apr-v1`'s class-A rule ("Every manifest leaf equals a value derived from a sha-bound source", `decide-apr-v1.yaml:146-148`).

**Falsification tests the planner must add (each observed RED under an induced negative):**
| Test | Prediction |
|------|------------|
| undeclared ECE failure | run copy with `ece_post` > 0.10, no `pilot_waiver` → `GateFailed` naming only `ece_post`, exit 3, nothing written |
| existing fail-closed vector | `demo.fail_closed_vectors` early_stopping `3d4b91da…` still refuses `GateFailed` (FALSIFY-LAYA-GATE-010; `tests/fail_closed_vectors.rs`) |
| waiver for other inputs | declaration's `train_jsonl_sha256` (or `recipe_id`) differs by one hex digit → `GateFailed` |
| margin never waived | margin fails (alone or with ECE) under a matching declaration → `GateFailed` naming `margin` |
| contract refuses `waives: [margin]` | policy construction error, before any run is read |
| report claims a waiver the contract lacks | `pilot_waiver.id` not in policy → refused (new variant, e.g. `WaiverUndeclared`) |
| manifest strip/forge | waived artifact with `/gate/waiver` removed or changed → rung-4 refusal at load; `verify_path` → `ManifestMismatch`/`ArtifactNotFromRun` |
| old artifacts byte-stable | re-pack of a run with no waiver → bytes identical to before the change (FALSIFY-DECIDE-APR-002 determinism); one real `just laya-verify` of `laya-antisemitism-v1.apr` stays `deploy_eligible: true` (≈ 21 min, `10-LEARNINGS.md:146`) |
| passing run + declaration | gate passes → accepted with `waived: None`, manifest has no `/gate/waiver` |

Add each to `laya-finetune-gate-v1` `falsification_tests` with `implemented_by`/`test:` lines in the same commit as the test (the contract's staged-binding rule, `laya-finetune-gate-v1.yaml:123-130`), plus `#[contract("laya-finetune-gate-v1", equation = "gate_pass")]`-style annotation on the new function.

### Q3. The folded spike: fitted T vs eval-optimal T, cheaply, without touching the gate

**Do not read `eval.jsonl` labels.** "Eval-optimal T" reads labels; computing it on the gate's eval split before the gate is declared is selection on the test set and breaks the one-declared-run discipline (`laya-finetune-gate-v1.yaml:18-22`: "A threshold chosen after seeing a result is not a gate"). Build the spike's data dir from the TRAIN split only.

**Recommended spike design (a measurement, never a gate run):**
1. Spike data: re-split the prepared `train.jsonl` (295) by the same declared rule (stratified, seed 13, 25%) into spike-train ≈ 220 (C 30 / S 49 / O 141) and spike-holdout ≈ 75 (C 11 / S 17 / O 47). [ASSUMED arithmetic from ceil rule; the script prints exact counts]
2. Harness: copy spike 027's `run_cell.py` (`.claude/skills/spike-findings-aprender/sources/027-laya-calibration-slice-and-tcap/run_cell.py`) into a new spike dir (e.g. `.planning/spikes/032-motivation-slice-and-tcap/`). It already drives `train.SeedRun.train_and_score` unchanged, keeps slice and eval logits, refits T unclamped / `[0.5, 5]` / `[0.5, 10]`, and finds an eval-oracle T on a grid (`run_cell.py` docstring lines 1-24). Add one hook: a `--fraction` passed to `data.calibration_split` (the contract value is untouched).
3. Grid: fractions {0.25, 0.35, 0.5} × seeds {13, 17, 23} × the declared recipe (early stopping, `--epochs` max per the declared rule) = 9 runs. Record per run: `t_fitted` (bounded and unbounded), `clamp_hit`, oracle T on spike-holdout, ECE at fitted T / at T = 5 / at oracle T, margin vs zero-shot on spike-holdout, best epoch.
4. Cost: ≈ 25-28 steps/epoch × ≈ 1.47 s/step (Phase 10's measured `s_per_step` 1.4695, `10-LEARNINGS.md:49`) ≈ 40 s/epoch; ≤ 12 epochs + scoring ≈ 10 min/run; ≈ 1.5 h of MPS for the grid. [ASSUMED: extrapolated from Phase 10's step time at a similar row length] Prune checkpoints (`SPIKE027_KEEP=0`-style): disk is 25 GiB free.
5. Declare BEFORE the spike runs: (a) the **clamp-binding rule** for D-18 (e.g. "binding if, at the chosen fraction, ≥ 2 of 3 seeds have unbounded `t_fitted` > 5.0, or ECE at T = 5 > 0.10 while ECE at the oracle T ≤ 0.10"); (b) the **slice-selection rule** (e.g. "keep 0.25 unless a larger fraction reduces the median |t_fitted − t_oracle| by ≥ X in ≥ 2/3 seeds without costing margin"). Both are the planner's to word; they must be committed before the first spike run.

**Existing tooling that applies:** `gate.fit_temperature(z, y, t_min, t_max)` and `gate.calibration_monitor` (`gate.py:65`, `:148`); `metrics.ece_top_label` (house ECE); `T_GRID` / `T_CURVE` in `run_cell.py`. The Rust clamp is `TEMP_MAX: f64 = 5.0` [VERIFIED: crates/aprender-decide/src/laya/temperature.rs:12-14 `pub const TEMP_MIN: f64 = 0.5;` / `pub const TEMP_MAX: f64 = 5.0;`], mirrored against `laya-finetune-gate-v1` `calibration_temp_max` by a test (`temperature.rs:146-147`).

**Correction to CONTEXT:** `calibration_temp_max` lives in `laya-finetune-gate-v1.yaml:157` (constants), not in `laya-parity-v1.yaml` (no match there). A cap change touches `laya-finetune-gate-v1` + `temperature.rs` (+ its mirror test) — and diverges from upstream Laya (`laya/common.py:376-377`).

**If the spike recommends a fraction ≠ 0.25 — the declaration is NOT a one-line edit.** Raising `constants.calibration_slice_fraction` globally raises Rust's minimum for EVERY run dir, so the shipped antisemitism-v1 and stance-64 run dirs would fail `check_slice_classes` (`verify.rs:1728`) the next time `laya-verify` runs — and `laya-deploy` and `laya-upload` run it before every AWS action (`justfile:1976-1993`). The safe mechanism is a per-run override keyed by data identity (e.g. a `calibration_slice_overrides:` entry `{train_jsonl_sha256, fraction}` read by `contract.py` and `VerifyPolicy`), or an OPTIONAL `recipe.json` field bound to a contract-declared set. Either is a contract + Python + Rust change with its own tests. Recommendation: write the selection rule so that 0.25 (already 75 slice rows here, versus the 12 that failed stance and the 48 that passed) is kept unless the spike shows a clear gain; plan the override only as a conditional branch.

### Q4. task.json for 3 labels; `--measure`; door; guide size

**Schema** [VERIFIED: contracts/decide-apr-v1.yaml:107-118]: `{type: "choice", instructions: string, criteria: object}`, `deny_unknown_fields`; criteria DOCUMENT order is the label index; 2 ≤ K ≤ `max_criteria` 510; options longer than the head are shrunk, refused only on marker loss. For this phase: criteria in the order `social_affiliative`, `conspiratorial`, `other` (CONTEXT domain; request `aprender.md:10`).

**Builder geometry** [VERIFIED: prepare_goldstandard.py:107-111 `ROW_MAX = 512`, `HEAD_MAX = 192`, `OPTION_MAX = 48`, `DOOR_CEILING = 8`]. Measured with the served tokenizer via `prepare_goldstandard.measure_task` (synthetic filler words, no data text) [VERIFIED: run this session]:

| Draft shape | P | head kept / raw | options kept (S/C/O) | notes |
|---|---|---|---|---|
| 1-word instruction, names only | 20 | 5/5 | 5/2/1 | fixed overhead ≈ 18 tokens |
| 40-word instruction, names only | 58 | 43/43 | 5/2/1 | |
| 40-word instruction, 8-word descriptions | 85 | 43/43 | 14/11/10 | ≈ a variant (a) shape |
| 120-word instruction, 12-word descriptions | 177 | 123/123 | 18/15/14 | ≈ a variant (b) shape |
| 400-word instruction, 12-word descriptions | **196 (cap)** | 142/403 (**head cut**) | 18/15/14 | `--measure` refuses a cut |

So variant (a) has ≈ 80 tokens for instruction + three descriptions (P ≤ 100); variant (b) tops out at P = 196 with ≈ 140 instruction tokens before a head cut. Texts per call (equal-length posts, `min(8, floor(800 / (P + t)))`): at P = 100: p50 5, p95 4, longest post 3; at P = 196: p50 3, p95 2, longest 2. No post is truncated at either P (max post 147 tokens; 196 + 147 = 343 < 512).

**Is a per-server lower `classify_max_texts` possible as configuration? No.** [VERIFIED: crates/aprender-mcp-decide/src/lib.rs:751-792]
```rust
pub fn check_served_task_fits(model: &Model, limits: &ClassifyLimits) -> pmcp::Result<usize> {
    ...
    let need = min_row.saturating_mul(limits.max_texts);
    if need > limits.max_total_tokens {
        return Err(refusal(format!(
            "{TOOL_NAME}: the served task's shortest built row is {min_row} tokens, so \
             classify_max_texts {} needs {need} > classify_max_total_tokens {} ({CONTRACT}); \
             this artifact's task cannot be served at the contracted tier", ...
pub fn build_server(model: Arc<Model>, name: &str, version: &str) -> pmcp::Result<Server> {
    build_server_with_limits(model, ClassifyLimits::CONTRACTED, name, version)
}
...
fn build_server_with_limits(
```
`build_server_with_limits` is private ("the only public door serves `ClassifyLimits::CONTRACTED`", `lib.rs:783-785`). With variant (b) (P > 100) the server **refuses to build** at cold start under the current contract. The contracted values [VERIFIED: contracts/decide-tool-boundary-v1.yaml:302, 311, 326 — `classify_max_texts: 8`, `classify_max_total_tokens: 800`, `served_task_min_row_tokens: 86`]. **Answer for D-09(b): it is a `decide-tool-boundary-v1` change** (e.g. making the served count `min(classify_max_texts, floor(800 / P))` derived per artifact, or a per-server table), with the compiled-constant mirror tests (`aprender-mcp-decide/src/tests.rs:188-199` `max_texts_fit_the_budget_at_the_shortest_row`), tool description, `probe --plan-only` and docs following. Phase 10 priced the same change as a MAJOR bump across the compiled constant, mirror test, four ledger rows and three documents (`10-LEARNINGS.md:35`).

**`served_task_min_row_tokens` is a single value naming the antisemitism task.** With two live tasks, (a) at P ≤ 100 changes nothing that fails (only `max_texts_fit_the_budget_at_the_shortest_row` reads it, and 8 × P ≤ 800 holds), but the comment now describes one of two servers. The planner should add an additive record of the motivation P (not overwrite 86) so the contract stays true for both.

**experiment-coach compatibility (new finding):** partner commit `52f31d7` ("split a batch the decider refuses for exceeding its 800-token request limit") makes the client split a batch in half and retry ONLY when the refusal text contains `classify_max_total_tokens`; any other refusal is surfaced as `Refused` [VERIFIED: `git show 52f31d7` in motivation-experiment, `crates/core/src/classifier.rs`]. A COUNT refusal (`classify_max_texts`) is not split. experiment-coach sends batches of 4 (request `aprender.md:24`). So: (a) at P ≤ 100 is fully compatible (count 4 ≤ 8; an over-budget batch is split); (b) is compatible only if the served count stays ≥ 4 — floor(800/196) = 4, so the derived door is exactly 4 at the cap and must never go lower. Put this in the door table the owner sees.

**`--measure` reuse:** `prepare_goldstandard.report()` reads posts through the antisemitism CSV reader (`read_rows`, header `["ID","Username","CreateDate","Biased","Keyword","Text"]`, `prepare_goldstandard.py:95, 285-289`), so it cannot take the motivation CSV. Import `measure_task`, `door_fields` and `contract_constant` from it (OPS-03) and supply post lengths from the new script's own reader; do not copy the functions.

**Latent defect to avoid copying:** `prepare_goldstandard.py --selftest-tokenizer` FAILS today, because it asserts the stance task's 57 equals the contract's `served_task_min_row_tokens`, which plan 10-11/10-12 moved to 86. Output this session:
```
  FAIL the stance task measures the contract's served_task_min_row_tokens 57 with 3 markers P 57 markers 3
1 case(s) failed
```
`prepare_motivation.py --selftest-tokenizer` must assert the stance control's P = 57 against a literal or the stance record, not the live constant (and the planner may fix the original in passing).

**Guide size in served-tokenizer tokens** (from the docx via `zipfile`): §4–6 = 9,478 Laya tokens; the head holds ≈ 140-178. Both variants are therefore deep condensations (≈ 1-2% of the text). Record that in the task evidence the way 10-01 recorded the paraphrase status.

### Q5. Kev arm (D-07): what exists, what blocks it

**What exists** [VERIFIED: git in `../aprender-016-upstream-sync`]: branch `spike/016-upstream-sync`, HEAD `32103ba83ce4165598ae31ddde89b5c9856b43f3` ("spike(020): batched Qwen3.5 prefill with a hidden-state readout for every position"), on top of `12976da0a` (hidden readout + MTP depth fix) and the upstream merge. Working tree has one modified file (`Cargo.lock`). `crates/aprender-serve/src/gguf/inference/forward/forward_qwen35_prefill.rs` exists. No binary in the worktree runs a Kev decision: the drivers are the separate spike crates `.planning/spikes/017-kev-rust-forward-parity/` and `020-qwen35-batched-prefill/`, which path-depend on the worktree (`020/Cargo.toml`: `realizar = { path = "/Users/guy/Development/machine-learning/aprender-016-upstream-sync/crates/aprender-serve", ... }`) and read a JSON fixture of **token ids** plus a `head.safetensors` (`020/src/main.rs`: `Row { ids, decide, opts, … }`, `Head::load`). There is no Rust tokenizer on this path ("Rust tokenizer: upstream has `gguf/byte_level_bpe.rs`. Port Kev's escaping…" is listed as open work, `references/qwen35-decision-inference.md:136-137`); rows are encoded in Python by `kev.model.encode` (`017/tools/export_and_oracle.py`).

**Pointer head for 3 labels:** none needed. The head scores each option's `</opt>` hidden state against `<decide>` (`logit_j = k(h_opt_j)·q(h_decide)/16`), so it is option-count agnostic; K = 3 is a `choice` question with three criteria (Kev `MAX_OPTIONS = 255`). "Fitting from train rows" means the spike-015 head-only adapters on frozen features: zero-shot, `bias` (K logit offsets, CPU < 1 s), `head_ft`, `logreg` (`references/kev-few-shot-evaluation.md:43-49`). Recommend **zero-shot + `bias`** (Kev-4B's best stance recipe, 0.607 → 0.642, and well calibrated, ECE 0.04-0.16), with a temperature refit on a train slice; `head_ft` is over-confident (ECE up to 0.37) without a refit. Fit in Python (`015/tools/fewshot.py`) on Rust-dumped `(h_decide, h_opts)` — training may stay in Python.

**Local artifacts** [VERIFIED: ls this session]: `~/.cache/huggingface/hub/models--jaredpalmer--kev-4b/` (adapter + `head.pt`, 305 MB) and `models--Qwen--Qwen3.5-4B-Base` (17 GB) are present. **No Kev-4B merged checkpoint and no Kev-4B GGUF exist**; only Kev-0.8B GGUFs (`017/models/kev-0.8b-merged-{f32,bf16,f16,q8_0}.gguf`, 2.9/1.4/1.4/0.8 GB) and the 0.8B merged dir (2.9 GB).

**Blockers / risks (flag each at an owner checkpoint):**
1. **Disk — blocking.** `df -h`: 25 GiB available of 926 GiB (98% used). A 4B fp32 merged HF dir (≈ 16-17 GB) and its f32 GGUF (≈ 16.8 GB, `references/` and memory) coexist during `convert_hf_to_gguf.py` → ≈ 34 GB transient, plus Laya runs (≈ 3 × 0.84 GB checkpoints, 0.8 GB artifact). Not feasible without freeing ≈ 15-20 GB (candidates the owner can choose: `models/decide/spike-027/`, the 0.8B GGUFs ≈ 6.5 GB, `models/decide/tweet-stance-16*` run dirs; `models/` totals 8.8 GB). [VERIFIED: df/du this session; size of the 4B fp32 files ASSUMED from 019's projection]
2. **4B never ran in Rust.** Parity and timing were proven at 0.8B only; 4B figures are projections ("expect ~2.5 s per short decision on 6 threads", `020/README.md:76`). The prefill is F32-only; BF16 falls back to the token-at-a-time path. Peak RSS for 0.8B f32 was 7.2 GB for a 3.0 GB file (`017/README` RUN-DTYPES); scaled naively, 4B could approach the 48 GB RAM ceiling. [ASSUMED] Merging 4B in fp32 via PEFT on CPU (`export_and_oracle.py` loads the checkpoint, then `AutoModelForCausalLM.from_pretrained` again) may need > 32 GB RAM. [ASSUMED]
3. **Guide length vs Kev's row limit.** [VERIFIED: vendor/kev/kev/model.py:13-15]
   ```
   MAX_STATE, MAX_BRANCH, MAX_PACKED = 384, 1024, 2048
   # serving context (kev.serve): per-branch cap mirrors Jev's ~32k, bounded by the base model window; longer than training, so untested there
   SERVE_MAX_STATE, SERVE_MAX_BRANCH = 8192, 8192
   ```
   and `encode(.., strict=True)` raises `ContextOverflow` when `len(br) > max_branch - len(S)` (`model.py:95-96`). §4–6 including §4.5 is 9,648 Qwen tokens → overflow. §4.1–4.4 + §5 + §6 (= CONTEXT's "about 4,500 words") is 6,811 tokens → fits, but at 6.6× the trained branch length, a regime Kev's author calls untested. The planner must state which text D-11's "full Sections 4–6" means (recommend: §4.1–4.4, §5, §6; exclude §4.5, the research-derivation narrative), and record that the regime is out of Kev's training distribution.
4. **No prefix reuse.** Kev's row is `[<state> post][<q> instructions <opt> … <decide>]` (`references/kev-few-shot-evaluation.md:34-35`): the post comes FIRST, so the ≈ 6.8k-token guide cannot be cached across rows; every row pays the full prefill.
5. **Compute.** Scaling the measured 0.8B prefill (915 tokens: 3.9 s at 6 threads, 3.0 s at 14; GEMM-bound, ~340 GFLOP/s) by ≈ 7× matmul work and ≈ 7.5× length, plus quadratic attention on the full-attention layers, gives roughly **2-5 min per row** for Kev-4B at ≈ 6.9k tokens on this 14-core M4 Pro. Scoring eval (100) + all train rows (295) for `bias` ≈ 13-33 h; eval + a stratified train subset (e.g. 60 rows) ≈ 5-13 h. [ASSUMED — extrapolation, not measured] CLAUDE.md makes > 1 h of compute on a non-lambda-vector host a check-in.

**Recommended Kev plan shape:** (i) owner checkpoint on disk, the guide-text definition and the compute budget; (ii) a timed probe of 2-3 rows (prefill only) with a rule declared before it (e.g. "run the full arm only if projected wall ≤ N h"); (iii) Python encodes rows from the Laya data dir via a small adapter (OPS-03: read `train.jsonl`/`eval.jsonl`, never re-split), writing ids + readout offsets only; (iv) Rust driver (a new spike-style crate path-depending on the worktree, like 020) dumps `h_decide`/`h_opts` and zero-shot probabilities; (v) Python fits `bias` + T on train rows, writes eval probabilities in the shared `eval_probs_schema` shape (`{rows: [{row, text_sha256, probabilities}], labels}`, `laya-finetune-gate-v1.yaml:464-469`); (vi) evidence records branch, SHA `32103ba8…`, the 4B GGUF sha256, timings, and the D-11 leakage check (eval-text exclusion from any in-prompt or fitting row). If (i) says no, the hand-off records the arm as blocked with the reason, exactly as Phase 10 recorded SC-4.

### Q6. SetFit arm (D-08)

**Recipe pieces** [VERIFIED: justfile:1345-1388; scripts/laya_train/setfit_cell_e1b16_cpu.json]: `laya_dir_to_canonical` adapter → `apr data select --shots 64 --seed 13` → `apr setfit train --config setfit_cell_e1b16_cpu.json` (epochs 1, batch 16, lr 2e-5, CPU, `root_seed` 13) → `apr eval --split validation --lock-out` → `apr eval --split test --selection-lock`. The adapter maps Laya `eval.jsonl` to `test` untouched and carves `validation` from TRAIN at `fraction: 0.1`, `seed: 13` [VERIFIED: laya_dir_to_canonical.rs:544-551].

**Changes needed before the cell can run:**
1. **The adapter refuses 3 labels.** [VERIFIED: laya_dir_to_canonical.rs:160-166]
   ```rust
    let names = doc.criteria.0;
    if names.len() != 2 {
        return Err(AdaptError::Data(format!(
            "task.json has {} criteria; this adapter writes the two-label cell only",
   ```
   and hardcodes the manifest identity `dataset: "goldstandard-2024"`, `target: "antisemitism"`, `task: "single-label two-class antisemitism classification"` (`laya_dir_to_canonical.rs:459-461`), plus `CARVE_DOMAIN "laya-goldstandard-validation-carve"` and default dirs (`:58-60`). Generalise to K ≥ 2 and take the identity strings as arguments (keep the antisemitism values as defaults so the Phase 10 adapted dir re-adapts byte-identically). SetFit itself handles 3 classes (the adapter's own doc notes the TweetEval bench is "three-class schema", `:6`).
2. **64 shots/class is impossible here.** [VERIFIED: aprender-contrastive-data/src/select.rs:59 `const ALLOWED_SHOTS: [u32; 4] = [8, 16, 32, 64];` and :222-229 refuse when a class pool < shots (`CrossSplitDuplicateUnderflow`)]. Train pools after the 0.1 carve (ceil): C 41 − 5 = 36, S 66 − 7 = 59, other 188 − 19 = 169. So **s32 is the largest legal cell** (s64 refuses on C and S). D-08's "reuses the Phase 5 / Phase 10 cell recipe" must become **s32e1b16** (same config, 32 shots) — an owner-visible deviation to record. It also means SetFit sees 96 of 295 train rows; the evidence must say "subsampled train" as 10-05 did.
3. **Macro-F1 needs per-row probabilities.** `apr eval` reports ACCURACY only (`justfile:1348`). Plan 10-06's `apr eval --split test --dump-probabilities FILE` was deferred (`10-LEARNINGS.md:72-77`; its PLAN exists: `10-06-PLAN.md`, files `crates/apr-cli/src/commands/eval/setfit.rs` etc.). D-05 ranks every arm by macro-F1 and reports false-assignment rates, so this phase must land 10-06's dump (or an equivalent single-pass output; OPS-03 forbids a second classify path).
4. **Memory.** Linear between the measured points (61 rows 9.4 GB, 241 rows 40.3 GB) gives ≈ 16 GB at 100 test rows [ASSUMED interpolation]; the validation carve (≈ 31 rows) is smaller. Confirm with the s8 rehearsal (D-08) using `/usr/bin/time -l`, before the s32 cell; keep a `blocked` verify branch (10-05's `failure_class: implementation`).
5. Phase 10 lesson: pass `--model-dir ~/.cache/aprender/minilm-l6-v2-1110a243` (holds `full_model.apr`); the HF snapshot does not.

### Q7. Thin crate, register, recipes, F-10-12-1

**Template** [VERIFIED: crates/aprender-mcp-decide-antisemite-lambda/Cargo.toml, src/main.rs]: `publish = false`, `[[bin]] name = "bootstrap" path = "src/main.rs"`, ONE dependency `aprender-mcp-decide-lambda = { path = "../aprender-mcp-decide-lambda" }`, `[lints] workspace = true`; `main.rs` calls `aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME"))`. Copy to `crates/aprender-mcp-decide-motivation-lambda/` (+ README linking the monorepo).

**Every file that must change for the new crate:**
| File | Change | Evidence |
|---|---|---|
| `Cargo.toml` (root) | add `"crates/aprender-mcp-decide-motivation-lambda"` to explicit `members` | antisemite entry at line 75 |
| `crates/aprender-core/tests/monorepo_invariants.rs` | add to `deployment_unit_bins` and raise `DEPLOYMENT_UNIT_BASELINE` 9 → 10 with a comment citing CONTEXT D-02 | [VERIFIED: :312-322 register, :445 `const DEPLOYMENT_UNIT_BASELINE: usize = 9;`, :443-444 "The NEXT model's thin crate is growth (9 -> 10) and needs a recorded CONTEXT decision with a blocking-human checkpoint."] |
| `scripts/duplicate_bin_names_allowlist.txt` | append `aprender-mcp-decide-motivation-lambda` to the `bootstrap` line (declared set must EQUAL observed) | [VERIFIED: the `bootstrap aprender-mcp-chronos-lambda aprender-mcp-decide-antisemite-lambda aprender-mcp-setfit-lambda aprender-setfit-train-lambda` line] |
| `README.md` | workspace crate count +1 (equality, FALSIFY-README-005) | `readme_contract.rs:80-106` |
| `crates/aprender-mcp-decide-lambda/src/tests.rs` | nothing required: `thin_server_crates_are_shims` discovers `aprender-mcp-decide-*-lambda` by directory | [VERIFIED: tests.rs:622-635] |
| `Cargo.lock` | regenerated | |
| `models/decide/deploy-config-aprender-mcp-decide-motivation.toml`, `models/decide/resolver-proof-aprender-mcp-decide-motivation.txt` | generated by `laya-deploy-config` / `laya-resolver-proof` (gitignored) | `justfile:1937-1938` |
| CLAUDE.md Decision-model row, `crates/aprender-mcp-decide-lambda/README.md` | name the second served model; every path cited in CLAUDE.md must exist (`test_documented_paths_exist`) | |
| `scripts/laya_gates.tsv` / `laya_claims.tsv` | only if a new guard (the health retry) gets a gate row; 10-12 edited both | `10-12-SUMMARY.md` key-files |

The just recipes already take the server id: `laya-build-bootstrap server`, `laya-deploy-config apr auth server env profile region`, `laya-deploy apr run data base server env profile`, `laya-upload`, `laya-grant`, `laya-deploy-verify apr server profile samples`, `laya-teardown` (`justfile:1701-2931`). `_laya-server-check` accepts `aprender-mcp-decide-motivation` (lower-case/digits/hyphens, ≤ 64 chars, `crates/<id>-lambda/Cargo.toml` with a `bootstrap` bin; `justfile:1574-1606`).

**F-10-12-1: no retry exists.** [VERIFIED: justfile:2068 `HEALTH="$(curl -fsS --max-time 30 "$HEALTH_URL")" || contain "GET $HEALTH_URL failed"`] — one GET, then containment (reserved concurrency 0). The existing retry is for the MCP probe only, and only for the 503 "Server is in error state" text [VERIFIED: justfile:2078-2094, `TRIES="${LAYA_EDGE_SETTLE_TRIES:-8}"`, 15 s sleep]. The plan must add a bounded retry on HTTP **404 only** for the edge `/health` GET (e.g. `LAYA_EDGE_HEALTH_TRIES`, default sized to the observed ≈ 8 min: 40 × 15 s), any other status or a body failing `_laya-edge-health-check` still contains at once; read the status with `curl -sS -o body -w '%{http_code}'` (not `-f`, which hides 404 from 500). Add the 404-then-200 / 404-forever / 500 cases to `laya-deploy-selftest`'s offline EDGE HEALTH table (shadowed `curl`), and keep F-10-12-2's documented recovery (re-run only the deploy step) as an owner-run branch.

**augury-dev (unresolved, must be confirmed by the owner):** the AWS profile `augury-dev` exists with `region = eu-west-1` [VERIFIED: ~/.aws/config], and Phase 11's intake names augury-dev "stage `prod`, region **eu-west-1**", a separate AWS account (`11-INTAKE/answers.md:61`). The working tree carries an UNCOMMITTED `justfile` diff that adds a `region` parameter to `laya-deploy-config` (rewrites the template's single `region = "us-east-1"` line) and `laya-weights-bucket` (adds `LocationConstraint`) — not yet to the other recipes, which inherit the region from `--profile`. The request document still writes the endpoint as `https://<server-id>.us-east.true-mcp.com/mcp` (`aprender.md:23`) and Phase 10 deployed with `ze-kasher-dev`/us-east-1. Open: profile, `env` (dev|prod; it is part of the bucket name `aprender-decide-weights-<account>-<env>`), region, the endpoint host, and whether pmcp.run deploys this server into the augury account. The weights bucket must be in the function's region or the 9 s S3 term of the cold start grows.

### Q8. Cold-start budget for a 3-label artifact with a longer prefix

- **Load is unchanged.** Weights, tokenizer and configs are identical in size (the task blob grows by a few hundred bytes). Phase 10: cold load ≈ 15.3-15.5 s = ≈ 9.0-9.2 s S3 download + ≈ 3.5 s sha256 + ≈ 2.8 s build (`10-HANDOFF.md:119`); contracted `cold_start_budget_ms: 15000`, `probe_budget_ms: 1100` (probes use the fixed decide-apr-v1 probe task, not the served task).
- **Per-call compute is bounded by tokens, not by P.** The budget is `classify_max_total_tokens` 800 × `tier_ms_per_token_at_512` 12 ms = 9.6 s, priced into `30000 − 15000 − 1100 − 4000` (`decide-tool-boundary-v1.yaml:307-311`); a longer P means fewer texts, not more tokens. Fewer, longer rows cost slightly more attention per token on the global-attention layers. [ASSUMED small; measure it]
- **What can actually eat the 4.5 s slack:** (1) a weights bucket in a different region from the function (augury-dev eu-west-1 — see Q7); (2) the Graviton generation lottery (`llm-endpoint-aws-deployment.md`: a 2B function landed on Graviton2); (3) a variant (b) door change that the per-token price was not re-measured for.
- Record cold (≥ 4 proven-cold samples of the concentrated and distributed maximal shapes via `laya-deploy-verify`) and warm timings, and the slack, exactly as Phase 10 did. Not gated.

### Q9. Evaluation metrics: where to compute them (OPS-03)

`scripts/laya_train/metrics.py` is the one Python home for gate metrics: `macro_f1`, `_f1`, `f_avg`, `ece_top_label`, `nll`, `rank_key` (functions at `metrics.py:54-105`), bit-identical with the Rust verifier on the frozen case file (`numeric_agreement`, `laya-finetune-gate-v1.yaml:508-553`). Nothing computes per-class precision/recall, false-assignment rates, length bands or bootstrap CIs today.

Recommendation:
- Add to `metrics.py` (pure numpy, with `--selftest` cases): `precision_recall_per_class(P, y)`, `false_assignment_rate(P, y, other_index, threshold)` (strict `>` per D-05 "exceeds"; numerator = eval `other` rows whose max over the non-`other` probabilities > threshold; denominator = eval `other` rows), `recall_at_threshold(P, y, k, threshold)`, and `ece_bootstrap_ci(P, y, bins, n_resamples, seed)` that calls `ece_top_label` (never a second ECE). Recommend a **class-stratified percentile bootstrap**, B = 10,000, `numpy.random.RandomState(13)`, reporting the 2.5/97.5 percentiles and the point estimate; stratified keeps the 14 C rows at 14 in every resample. [ASSUMED: method choice is Claude's discretion]
- Add a new `scripts/laya_train/bakeoff.py` that reads THREE probability files in `eval_probs_schema` shape (Laya's shipped `eval-probs.json` and the three `seeds/seed-<s>/eval-probs.json`; the SetFit dump converted to that shape; the Kev dump) plus `eval.jsonl` for labels and character lengths, and writes counts-only JSON: per arm macro-F1 (ranking), per-class P/R, false-assignment and recall at thresholds {0.5, 0.6, 0.7, 0.85}, ECE with CI, and the same per length band. Length bands: tertile cut points of eval character lengths at the 33.33/66.67 percentiles, declared rule `band = 0 if len ≤ c1, 1 if len ≤ c2, else 2`, with each band's per-class counts printed beside every rate (≈ 33 rows and ≈ 4-5 C per band — say so). Label it "reported, not a gate".
- The Laya gate numbers (margin, ECE, macro-F1) stay the Rust-recomputed values from pack/verify; bakeoff.py must reproduce them bit-for-bit from the same files (assert it).

### Q10. Hand-off shape and evidence conventions

`10-HANDOFF.md` sections (follow them, renumbered as needed): 1 Identity (artifact, sha256, labels in order, recipe_id, base, method/variant, verify line); 2 Endpoint and auth posture; 3 Items for the partner to confirm (transport, batch size, texts per call, label order); 4 The task (verbatim instructions and criteria, `task_json_sha256`, measured P, limits table); 5 Data and gate (counts, CSV sha, per-seed table, shipped seed, claim scope quoted from `eval_set.claim`, disclosures); 6 Comparison record; 7 Acceptance replay; 8 Timings (reported, not gated); 9 Accuracy readout; 10 Dates and conditions; 11 Scope. Every number names its record.

Phase 12 additions the hand-off needs: **lead with the false-assignment rate** per arm and per length band (CONTEXT specifics); the gate-failed-pilot statement and ECE CI if D-19 fired; PILOT labelling and the guide's draft status/sha; D-22's prior correction (π_train from train counts: other 188/295 = 0.637, S 66/295 = 0.224, C 41/295 = 0.139; π_field from Table 7 ≈ S 0.07, C 0.09, other ≈ 0.84 — the guide's percentages are of 1,546 posts and "clear evidence of one or both", so mixed posts make the field figures approximate [ASSUMED reading]; and that no server change was made); D-15's UNMEASURED source shortcut; the measured door for the chosen variant and the experiment-coach split behaviour (Q4); the Kev and SetFit arm status (blocked branches allowed); and the caveat that the motivation posts are GoldStandard 2024 posts, the corpus antisemitism-v1 was trained on, so stage-1 + stage-2 pipeline numbers on these posts are not out-of-sample.

Evidence JSON conventions from Phase 10: top-level `schema` (versioned string, e.g. `laya-task-evidence-v2`), `phase`, `plan`, `measured_on`, `commit`/`measured_at_commit`, explicit `outcome` with a closed value set, owner reply/decision fields verbatim (`owner_reply`, `owner_approved_on`), hashes not text, and an explicit `blocked`/`skip` branch rather than missing keys (`10-TASK-EVIDENCE.json`, `10-GATE-RUN-EVIDENCE.json`, `10-PROBE-EVIDENCE.json`, `10-SETFIT-CELL-EVIDENCE.json`, `10-ACCEPTANCE-REPLAY.json` keys read this session). Suggested names: `12-TASK-EVIDENCE.json`, `12-SPIKE-EVIDENCE.json`, `12-PROBE-EVIDENCE.json`, `12-GATE-RUN-EVIDENCE.json`, `12-SETFIT-CELL-EVIDENCE.json`, `12-KEV-ARM-EVIDENCE.json`, `12-BAKEOFF-EVIDENCE.json`, `12-DEPLOY-EVIDENCE.json`, `12-ACCEPTANCE-REPLAY.json`, `12-HANDOFF.md`.

## Architecture Patterns

### System Architecture Diagram

```
partner CSV + guide docx (read in place, pinned sha256)
        │
        ▼
prepare_motivation.py ──(counts+sha only)──► data/decide/motivation-v1/{task.json, train.jsonl, eval.jsonl}
        │  --measure (served tokenizer)                         │
        ▼                                                       │
 door table (P, texts/call) ──► OWNER CHECKPOINT: task bytes ───┤
                                                                │
        ┌───────────────────── train.jsonl only ────────────────┤
        ▼                                                       │
 spike (train-only holdout): T_fitted vs T_oracle × fraction    │
        │ binding? ──yes──► OWNER STOP (D-18)                   │
        ▼ no                                                    │
 probe_epoch (1 epoch, timed) ─► declared epochs rule           │
        ▼                                                       │
 COMMIT contract: slice decision + pilot_waiver(applies_to = data shas + recipe_id)
        ▼                                                       │
 train.py ×3 seeds (MPS) ─► run dir (gate-report pass?, eval-probs, rescore-noise)
        │ margin fail ──► OWNER HALT (D-20)                     │
        ▼                                                       │
 pack_laya pack ──► verify_loaded: re-score ─► recompute gate ─► {pass | waived(ece only, declared) | GateFailed}
        ▼ deploy_eligible                                        │
 thin crate + laya-deploy (health 404 retry) ─► pmcp.run server ─► acceptance replay
                                                                │
 SetFit arm: adapter(K=3) ─► apr data select s32 ─► setfit train ─► apr eval --dump-probabilities ─┐
 Kev arm (owner-gated): kev encode ids ─► Rust prefill_hidden (worktree) ─► bias+T fit ────────────┤
 Laya eval-probs ──────────────────────────────────────────────────────────────────────────────────┤
                                                                                                    ▼
                                                    bakeoff.py (metrics.py) ─► 12-BAKEOFF-EVIDENCE ─► 12-HANDOFF.md
```

### Recommended file layout (new/changed)
```
scripts/laya_train/
├── prepare_motivation.py     # new: copy of prepare_goldstandard pattern; imports its measure helpers
├── metrics.py                # + per-class P/R, false-assignment, recall@t, ece_bootstrap_ci (+ selftest)
├── bakeoff.py                # new: three-arm counts-only report
├── train.py / contract.py    # + OPTIONAL gate-report pilot_waiver
└── probe_epoch.py            # GUARD_DEFAULT / EPOCHS_RULE are Phase-10 literals: parameterise or pass --guard
crates/aprender-decide/src/{verify.rs, pack.rs, artifact.rs, artifact/ladder.rs, verify/tests.rs}
crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs   # K >= 2, identity args
crates/apr-cli/src/commands/eval/setfit.rs                           # --dump-probabilities (10-06)
crates/aprender-mcp-decide-motivation-lambda/{Cargo.toml, src/main.rs, README.md}
contracts/laya-finetune-gate-v1.yaml   # pilot_waivers, gate_report_schema, run_field_bindings, falsification_tests
contracts/decide-apr-v1.yaml           # manifest.fields gate.waiver + bindings row
justfile                               # laya-deploy health 404 retry (+ selftest rows); a laya-prepare-motivation wrapper
.planning/spikes/032-motivation-slice-and-tcap/   # spike harness (copied from 027)
```

### Pattern: declare before you read
Every value that could be tuned after seeing a result (eval fraction, slice fraction, epochs, waiver, clamp-binding rule, Kev budget) is written down and committed BEFORE the step that reads labels. The order is the plan's spine: prepare → owner task approval → spike (train-only) → probe → declare (contract commit) → gate run.

### Anti-Patterns to Avoid
- **Spike on eval.jsonl:** reading eval labels to pick a slice size makes the gate run a second look at its own test set.
- **Editing `calibration_slice_fraction` globally:** breaks re-verification of shipped artifacts (Q3).
- **Serializing a new manifest field as `null`:** breaks `verify_path` byte binding of existing artifacts (Q2).
- **A per-server door "flag" that bypasses `ClassifyLimits::CONTRACTED`:** the contract and the tool description would no longer describe the served server.
- **Copying `prepare_goldstandard.report()`:** it reads only the antisemitism CSV header; import the pure helpers.
- **Quoting `tokens_planned == texts × P`** in any verify (carried lesson).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| ECE / macro-F1 | a new ECE in bakeoff.py | `metrics.ece_top_label`, `metrics.macro_f1` | Bit-identical with the Rust verifier (`numeric_agreement`) |
| Gate decision | a Python "waiver pass" | Rust `verify_loaded` + contract policy | The Rust verifier is the authority; Python reports never decide eligibility |
| Calibration slice | a second split function | `data.calibration_split` (with a spike-only fraction argument) | Same grouping/seed semantics Rust re-derives |
| Temperature fit | a new optimiser | `gate.fit_temperature` | Bounded fit identical to production |
| Text normalisation | a new normalizer | `data.normalize` / `normalized_sha256` (nfc-trim-ws-v1) | Rust mirrors it (`verify.rs:1638-1645`) |
| Write-once output | new file logic | `prepare_stance.write_once` | Already proven byte-identical re-runs |
| Task prefix measurement | a new builder | `laya.common.build_sequence` via `prepare_goldstandard.measure_task` | The served builder's geometry |
| SetFit split / selection | a Python SetFit | adapter + `apr data select/setfit train/eval` | Attested, seeded, pinned path |
| Kev rows | a Rust tokenizer port | `kev.model.encode` in Python → ids JSON | Rust tokenizer does not exist on this path |
| Edge health/identity | new probe | `_laya-edge-health-check`, `aprender-mcp-decide-lambda --example probe` | Existing chain with selftests |

**Key insight:** this phase's risk is not missing code; it is changing a global policy (contract constant, door, manifest shape) in a way that silently re-decides artifacts that are already live.

## Runtime State Inventory

Not a rename/refactor phase for existing names, but it adds a live server next to a live one. What carries state outside git:

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | Weights bucket `aprender-decide-weights-<account>-<env>` holds `decide/aprender-mcp-decide-antisemite/<sha>.apr` and the retired prefix's object (10-12-SUMMARY) | New prefix `decide/aprender-mcp-decide-motivation/<sha>.apr` via `laya-upload`; nothing else touched |
| Live service config | pmcp.run server `aprender-mcp-decide-antisemite` (live, must stay byte-stable); new server `aprender-mcp-decide-motivation` (created by cargo-pmcp) | Do not redeploy the antisemite server; any shared-library change must keep its artifact verifiable |
| OS-registered state | None — verified: no launchd/cron involvement in these recipes | none |
| Secrets/env vars | Lambda env `APRENDER_DECIDE_S3_URI`, `APRENDER_DECIDE_SHA256` per server config (template) ; pmcp.run login token (≈ 1 h) | Generated per server by `laya-deploy-config`; owner logs in right before deploy |
| Build artifacts | `target/<arm64>/release/bootstrap` is shared by every thin crate (`laya-build-bootstrap` guards against a stale one by grepping `<server>-lambda`) | Always rebuild via the recipe; never reuse a bootstrap |

## Common Pitfalls

### Pitfall 1: The spike reads the gate's eval labels
**What goes wrong:** slice size chosen on the eval split; the declared run is no longer blind. **Avoid:** spike-holdout carved from train. **Warning sign:** the spike script opens `eval.jsonl`.

### Pitfall 2: A global contract edit re-decides shipped artifacts
**What goes wrong:** raising `calibration_slice_fraction` or serializing `gate.waiver: null` makes `laya-verify` refuse `laya-antisemitism-v1.apr`. **Avoid:** per-run overrides; `skip_serializing_if`. **Warning sign:** `pv diff` on the gate contract changes a `constants:` value; run one real `just laya-verify` of the antisemitism artifact after the change.

### Pitfall 3: Variant (b) silently cannot start
**What goes wrong:** P > 100 under the current contract → `check_served_task_fits` refuses at build → cold start fails, containment. **Avoid:** (b) only with a `decide-tool-boundary-v1` change in the same plan; `probe --plan-only` offline before deploy.

### Pitfall 4: A door below 4 breaks experiment-coach
**What goes wrong:** count refusals are not split by the client (only `classify_max_total_tokens` ones are). **Avoid:** never serve fewer than 4 texts.

### Pitfall 5: The ECE on 14 conspiratorial rows is noise
**What goes wrong:** 100 eval rows in 15 bins; one MPS replicate moved ECE by up to 0.048 on 459 rows (spike 027). **Avoid:** report the bootstrap CI beside every ECE; the median-ECE seed rule already reduces variance.

### Pitfall 6: The model learns length
**What goes wrong:** medians 243/198/152 chars; tertile 3 is S-heavy. **Avoid:** per-band per-class P/R with counts; state band sizes.

### Pitfall 7: SetFit at s64 refuses; s32 is a recipe deviation
**What goes wrong:** `CrossSplitDuplicateUnderflow` at s64. **Avoid:** declare s32e1b16 before the cell; record "subsampled train (96 of 295)".

### Pitfall 8: Kev disk/time
**What goes wrong:** 4B merge + GGUF fill the disk mid-convert; per-row minutes turn into a day. **Avoid:** owner checkpoint with free-space proof (`df`) and a timed probe with a declared go/no-go rule.

### Pitfall 9: Health 404 containment on the new id
**What goes wrong:** first `/health` 404 → reserved concurrency 0 (F-10-12-1). **Avoid:** bounded 404-only retry, selftested offline; F-10-12-2 recovery by re-running only the deploy step.

### Pitfall 10: probe_epoch defaults are Phase 10's
**What goes wrong:** `GUARD_DEFAULT = "models/decide/laya-antisemitism-v1"` and `EPOCHS_RULE = "4 unless s_per_step <= 1.0, then 6 (RESEARCH Q4)"` (`probe_epoch.py:61-65`) — a rule written for 8.5k rows where time was the constraint. **Avoid:** pass `--guard models/decide/laya-motivation-v1` and write a new epochs rule for ≈ 220 fit rows (time is ≈ minutes, so the rule should be about calibration, e.g. the contract maximum 12 under early stopping, as spike 027's recommended s64 recipe used; Phase 10's seeds hit training loss 0 by epoch 3 and two restored epoch 1, `10-LEARNINGS.md:138`). Production at > 16 shots/class REQUIRES `--epochs` in [4, 12] (`contract.py:216-241`); shots_per_class here = 188.

### Pitfall 11: `prepare_goldstandard --selftest-tokenizer` is already red
See Q4. Do not copy the assertion against the live constant.

## Code Examples

### Prepare-script constants for this dataset (pattern from prepare_goldstandard.py:92-111)
```python
# Source: scripts/laya_train/prepare_goldstandard.py (pattern); values measured this session
CSV_HEADER = ["Post (English, original)", "Class"]
LABELS = ["social_affiliative", "conspiratorial", "other"]          # criteria document order = label index
LABEL_OF = {"Social-affiliative": 0, "Conspiratorial explanation": 1, "None": 2}
SPLIT_SEED = 13
EVAL_DIVISOR = 4                                                    # RULE split: 0.25 = 1 / 4, ceil in integers
PINNED = {"rows_in": 395, "distinct_texts": 395, "duplicates_collapsed": 0,
          "mixed_label_groups_dropped": 0, "mixed_label_rows_dropped": 0,
          "csv_sha256": "bc61c76bd29bcdd4c6466266b4e1161841f2107a4f02f5ff4fd96f1b7a3a4ae7"}
DEFAULT_OUT = REPO / "data" / "decide" / "motivation-v1"            # under gitignored /data/
```
Expected output counts: train {social_affiliative 66, conspiratorial 41, other 188}, eval {23, 14, 63}.

### Waiver decision (shape, in verify_loaded) — sketch
```rust
// Source: shape derived from crates/aprender-decide/src/verify.rs:3266-3301 (read this session)
let clauses = check_gate(&inputs.gate_report, &recomputed, n, policy)?;
let waived = if clauses.is_empty() {
    None
} else if clauses == [GateClause::EcePost] {
    policy.matching_pilot_waiver(&inputs.gate_report)   // id + applies_to + waives == [ece_post]
          .ok_or_else(|| gate_failed(clauses.clone(), ...))?   // undeclared -> GateFailed, as today
          .into()
} else {
    return Err(gate_failed(clauses, ...));               // margin never waivable (D-20)
};
// ... seed policy check unchanged ...
Ok(VerifyReport { deploy_eligible: true, waived, .. })
```
(`matching_pilot_waiver`, `gate_failed` and `waived` are names for the planner to choose. [ASSUMED])

### Health retry shape (justfile, bash)
```bash
# Source: pattern of justfile:2078-2094 (the 503 retry), applied to the edge GET at :2068
H_TRIES="${LAYA_EDGE_HEALTH_TRIES:-40}"; h=1
while :; do
    code="$(curl -sS --max-time 30 -o "$HBODY" -w '%{http_code}' "$HEALTH_URL")" || contain "GET $HEALTH_URL failed (transport)"
    [ "$code" = "200" ] && break
    if [ "$code" = "404" ] && [ "$h" -lt "$H_TRIES" ]; then h=$((h + 1)); sleep 15; continue; fi
    contain "GET $HEALTH_URL returned $code after $h attempt(s)"
done
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| One server id `aprender-mcp-decide` | One thin crate + id per dataset | Phase 10 D-19 (2026-10-22) | This phase adds the second |
| 1e-5 fixed re-score bar | `max(1e-5, 4 × torch32-vs-f64 noise)` (A1) | laya-parity-v1 2.0.0 (plan 08-13) | Pack re-score needs no new work |
| Declared seed ships | Median-ECE of seeds 13/17/23 | gate 1.4.0 (2026-09-27) | Unchanged |
| f32 gate metrics | f64 exactly-summed, bit-identical Py/Rust | gate 4.0.0 (plan 08-27) | bakeoff.py must reuse metrics.py |
| experiment-coach fails on token refusal | Splits on `classify_max_total_tokens` refusals | partner `52f31d7` (2026-10-25) | Door ≥ 4 suffices for its batches of 4 |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Spike grid ≈ 1.5 h MPS (extrapolated from Phase 10's 1.4695 s/step) | Q3 | Schedule only |
| A2 | Gate run with ≈ 220 fit rows, ≤ 12 epochs, 3 seeds ≈ ≤ 1 h | Pitfall 10 | Schedule only |
| A3 | Kev-4B fp32 merged + GGUF ≈ 34 GB transient on disk; peak RAM could near 48 GB | Q5 | Kev arm fails mid-run |
| A4 | Kev-4B ≈ 2-5 min per 6.9k-token row on 14 M4 threads | Q5 | Arm takes days, or is faster than feared |
| A5 | `apr eval` ≈ 16 GB at 100 test rows (linear interpolation) | Q6 | OOM → blocked branch |
| A6 | A longer P adds little per-call latency (token budget bounds work) | Q8 | Cold slack shrinks |
| A7 | π_field ≈ S 0.07 / C 0.09 / other 0.84 read from guide §10.2 (overlapping categories) | Q10 | Mis-stated client reweighting reference |
| A8 | Stratified percentile bootstrap, B = 10,000, seed 13 is an adequate ECE CI | Q9 | CI width misleading |
| A9 | Spike-holdout counts C 11 / S 17 / O 47 from re-applying the 25% rule to train | Q3 | Minor |
| A10 | The waiver function/field names in the code sketch | Code Examples | None (naming) |
| A11 | augury-dev means profile `augury-dev`, eu-west-1, a separate account | Q7 | Wrong account/region deploy; cold-start slack |

## Open Questions (RESOLVED)

Every question below is closed; the pointer names where (owner amendments in 12-CONTEXT.md, or the plan that decides it).

1. **Where does "augury-dev" deploy?** Known: profile `augury-dev` = eu-west-1; an uncommitted region parameter exists in two recipes; the request's endpoint host says `us-east`. Unclear: env (dev|prod), account, endpoint host, whether cargo-pmcp's pmcp.run target deploys into that account. Recommendation: an owner question before the deploy plan; extend `region` to every AWS-touching recipe that needs it, with selftests. **RESOLVED → D-23** (owner, 2026-10-26): deploy with profile ze-kasher-dev in us-east-1, the antisemite server's account, bucket and host pattern; the uncommitted region parameter is not this phase's work (plans 12-01, 12-04, 12-10 commit only their own justfile hunks; 12-12 proves the deploy does not depend on that diff).
2. **Kev arm go/no-go.** Disk (needs ≈ 15-20 GB freed), compute (multi-hour), guide-text definition (exclude §4.5?), and D-07's Rust-only rule. Recommendation: owner checkpoint after a timed probe; allow a recorded `blocked` outcome. **RESOLVED → D-24**: Kev-4B goes ahead after `cargo clean`, guide §4-6 without §4.5, a timed row and a budget checkpoint, or a recorded blocked arm (plans 12-14, 12-15).
3. **SetFit shots.** s64 is infeasible; s32 is the largest legal cell. Recommendation: declare s32e1b16 at the start of the SetFit plan and record the deviation. **RESOLVED → D-25**: the s32 cell as a declared deviation, the K-label adapter and `apr eval --dump-probabilities` (plans 12-06, 12-10).
4. **Variant (b) mechanism.** If the owner picks (b), the door change is a `decide-tool-boundary-v1` MAJOR change; recommend deriving the served count per artifact (`min(8, floor(800/P))`, never below 4) rather than a per-server table, so the antisemite server's behaviour is unchanged (P 86 → 8). **RESOLVED → D-26**: a per-artifact door through a decide-tool-boundary-v1 change, only if the owner picks (b) at the D-09 checkpoint (plan 12-09 branch b).
5. **Slice-fraction override mechanism**, only if the spike recommends ≠ 0.25 (Q3). **RESOLVED → plan 12-08**: a no-op at 0.25, otherwise a per-run override keyed by the motivation train sha, read by both Rust and Python, never a global edit.
6. **Epochs rule** for ≈ 220 fit rows — to be written before the probe (Pitfall 10). **RESOLVED → plan 12-07's rule**: the `motivation-v1` entry of probe_epoch.py's rule table (E = 12 under early stopping unless the projected three-seed hours exceed 6), committed before the timed probe.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| uv + `scripts/laya_train/.venv` | back office | ✓ | uv 0.9.5 | — |
| Laya-en snapshot 55cf4c4e (tokenizer, weights) | train, measure, verify | ✓ | — | — |
| MPS (M4 Pro, 48 GB RAM, 14 cores) | training | ✓ | — | CPU (flagged) |
| Free disk | everything; Kev arm critical | ⚠ 25 GiB | — | owner frees space |
| cargo-zigbuild | bootstrap build | ✓ | 0.23.4 | — |
| cargo-pmcp | deploy | ✓ | 0.28.1 | resolver proof per server |
| aws CLI + profiles `ze-kasher-dev`, `augury-dev` | deploy | ✓ | 2.29.0 | — |
| pv | contract validate/diff | ✗ on PATH; `target/release/pv` built | — | `cargo build --release -p aprender-contracts-cli` |
| pmat | code search | ✓ | 3.15.0 | — |
| bashrs | shell lint | ✓ | 6.66.3 | — |
| just | recipes | ✓ | 1.46.0 | — |
| MiniLM `full_model.apr` | SetFit arm | ✓ | 1110a243 | — |
| Kev-4B adapter + Qwen3.5-4B base (HF cache) | Kev arm | ✓ | kev @ 7405b72 | — |
| Kev-4B merged / GGUF | Kev arm | ✗ | — | must be built (disk!) |
| llama.cpp converter | Kev arm | ✓ (vendor) | — | — |
| Worktree `../aprender-016-upstream-sync` | Kev arm | ✓ | 32103ba8 | — |

**Missing dependencies with no fallback:** free disk for the Kev arm (blocking for that arm only).
**Missing dependencies with fallback:** `pv` (use the in-tree build).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `cargo test` / nextest; Python self-tests (`--selftest`) under uv; `just` selftest recipes; `pv validate` |
| Config file | `Cargo.toml` workspace; `scripts/laya_train/pyproject.toml`; `ci/explicit-test-commands.d/` (030 monorepo invariants, 040 readme contract) |
| Quick run command | `cargo test -p aprender-decide --lib` ; `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest` |
| Full suite command | `just laya-train-selftest && just laya-deploy-selftest && cargo test -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda --lib && cargo test -p aprender-core --test monorepo_invariants --test readme_contract && bash scripts/check_duplicate_bin_names.sh` |

### Phase Requirements → Test Map
| Decision | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| D-16/D-13 | split rule, pins, no text, 25% counts {23,14,63} | unit (synthetic CSV + sentinel) | `python -I scripts/laya_train/prepare_motivation.py --selftest` | ❌ Wave 0 |
| D-09 | measured P / door table for both variants, head/option cut refusal | tokenizer selftest + `--measure` | `prepare_motivation.py --selftest-tokenizer`; `--measure --task <draft>` | ❌ Wave 0 |
| D-10 | approved bytes == data dir task.json == artifact `inputs_sha256.task_json` | evidence check | `shasum -a 256 data/decide/motivation-v1/task.json` vs `12-TASK-EVIDENCE.json` and `pack_laya inspect` | — |
| D-17/D-18 | spike reads no eval.jsonl; binding rule applied | spike assertion + evidence | spike script refuses an `eval.jsonl` path; `12-SPIKE-EVIDENCE.json` `outcome` | ❌ Wave 0 |
| D-19 | declared waiver honoured; undeclared refused; margin never waived; old artifacts byte-stable | unit (verify/tests.rs) | `cargo test -p aprender-decide --lib verify::tests::` (new waiver cases) + `--test fail_closed_vectors` (armed) | ❌ Wave 0 |
| D-19 | contract schema | contract | `target/release/pv validate contracts/laya-finetune-gate-v1.yaml contracts/decide-apr-v1.yaml` | ✅ |
| D-19 | Python report field | lifecycle | `just laya-train-selftest` (python_refusals sweep, expected_keys) | ✅ (extend) |
| D-20 | margin failure → GateFailed exit 3, plan halts | unit + verify branch | as D-19 | ❌ Wave 0 |
| D-05/D-14/D-22 | P/R, false-assignment@{0.5,0.6,0.7,0.85}, tertiles, ECE CI; bakeoff reproduces Rust gate numbers bit-for-bit | unit | `python -I scripts/laya_train/metrics.py --selftest`; `bakeoff.py --selftest` | ❌ Wave 0 |
| D-08 | adapter K=3; Phase 10 dir re-adapts byte-identically; s32 select; dump probabilities | unit + rehearsal | `cargo test -p aprender-contrastive-data --examples`; `cargo test -p apr-cli --lib eval::setfit`; s8 rehearsal with `/usr/bin/time -l` | ❌ Wave 0 |
| D-07/D-11 | Kev branch SHA recorded; leakage check 0 eval texts in prompt/fit rows | evidence assertion | `python -I` check over `12-KEV-ARM-EVIDENCE.json` | ❌ Wave 0 |
| D-02 | register 10, allowlist exact, thin crate is a shim, README count | integration | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh`; `cargo test -p aprender-mcp-decide-lambda --lib tests::thin_server_crates_are_shims`; `cargo test -p aprender-core --test readme_contract` | ✅ (edit) |
| F-10-12-1 | 404→200 retried; 404-forever and 500 contained | offline selftest (shadowed curl) | `just laya-deploy-selftest` | ✅ (extend) |
| Carried | live identity, labels order, sums to 1, sha == pin; cold/warm reported | live (human checkpoint) | `just laya-deploy-verify <apr> aprender-mcp-decide-motivation <profile>`; acceptance replay | ✅ |
| Gate | one declared run, device from trace, rc file | evidence | `grep 'DEVICE seed=13 requested=mps used=' <log>`; `cat <rc file>` | — |

### Sampling Rate
- **Per task commit:** the quick command for the touched module (`cargo test -p aprender-decide --lib`, or the script's `--selftest`).
- **Per wave merge:** the full suite command above.
- **Phase gate:** full suite green, one real `just laya-verify` of BOTH `laya-antisemitism-v1.apr` (byte-stability) and the motivation artifact, before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `scripts/laya_train/prepare_motivation.py` with `--selftest` / `--selftest-tokenizer` (sentinel no-text test)
- [ ] waiver cases in `crates/aprender-decide/src/verify/tests.rs` (+ manifest strip/forge in artifact tests)
- [ ] metrics.py new functions + selftest cases; `bakeoff.py --selftest`
- [ ] adapter K≥2 tests; `--dump-probabilities` tests (from 10-06-PLAN)
- [ ] spike harness guard that refuses `eval.jsonl`
- [ ] `laya-deploy-selftest` EDGE HEALTH 404-retry rows

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (auth off by owner decision, as Phase 10 D-13) | record the cost exposure (each cold call ≈ 25 s of 10 GB compute) |
| V3 Session Management | no | — |
| V4 Access Control | yes (AWS) | stack-declared `s3:GetObject` on `decide/<server>/*` only (`_laya-iam-check`), read-only `sts`/`expires_at` checks, executor never refreshes credentials |
| V5 Input Validation | yes | existing door (`classify_max_texts`, bytes, tokens) and `deny_unknown_fields`; prepare script refuses malformed CSV by row number, never echoing content |
| V6 Cryptography | yes (integrity only) | sha256 pins via `shasum`/`sha2`; never hand-rolled |
| V7 Error handling / logging | yes | no post text in logs, evidence, refusals (sentinel test); refusals name bounds, not inputs |
| V10/V14 Configuration & supply chain | yes | content-addressed weights key, sha-pinned config, resolver proof per cargo-pmcp version; waiver declared in a `pv diff`-visible contract |

### Known Threat Patterns
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Deploying a gate-failed model without declaration | Tampering / Elevation | Rust verify honours only a contract-declared waiver bound to data shas + recipe_id; falsification tests |
| Editing a gate report to claim a waiver | Tampering | waiver id must exist in the contract policy; report blob is sha-bound in the artifact |
| Artifact substitution in S3 | Tampering | `APRENDER_DECIDE_SHA256` pin, content-addressed key, live identity probe |
| Post text leaking to git/logs | Information disclosure | gitignored `/data/`, counts-only output, sentinel selftests |
| Wrong binary under the new id | Spoofing | compile-log check, `bootstrap` names `<server>-lambda`, edge `serverId`, identity probe |
| Open endpoint cost abuse | Denial of service | accepted by owner (auth off); door bounds cap per-call work |

## Sources

### Primary (HIGH confidence) — read this session
- `scripts/laya_train/{prepare_goldstandard.py, contract.py, data.py, train.py, probe_epoch.py, metrics.py}`
- `contracts/{laya-finetune-gate-v1.yaml, decide-apr-v1.yaml, decide-tool-boundary-v1.yaml}`
- `crates/aprender-decide/src/{verify.rs, pack.rs, artifact.rs, laya/temperature.rs}`, `examples/pack_laya.rs`
- `crates/aprender-mcp-decide/src/{lib.rs, tests.rs}`, `crates/aprender-mcp-decide-lambda/src/{tests.rs, probe.rs}`, `.pmcp/deploy.toml.template`
- `crates/aprender-mcp-decide-antisemite-lambda/*`, `crates/aprender-core/tests/monorepo_invariants.rs`, `scripts/duplicate_bin_names_allowlist.txt`
- `crates/aprender-contrastive-data/{examples/laya_dir_to_canonical.rs, src/select.rs}`
- `justfile` (laya-* recipes, uncommitted diff)
- Phase 10: `10-LEARNINGS.md`, `10-HANDOFF.md`, `10-12-SUMMARY.md`, `10-06-PLAN.md`, evidence JSON keys
- Skill references: `laya-calibration-gate-and-pack-tolerance.md`, `qwen35-decision-inference.md`, `kev-few-shot-evaluation.md`; spike 027 `run_cell.py`
- Kev vendor `kev/model.py` @ 7405b72; worktree git log; spike 017/020 sources
- Partner repo: `docs/requests/aprender.md` @ 0c46b68, commit `52f31d7`, CSV and guide docx (counts/tokens only)

### Secondary (MEDIUM confidence)
- Phase 11 intake notes on augury-dev (`11-INTAKE/answers.md:61`)

### Tertiary (LOW confidence)
- Kev-4B timing/memory/disk projections (extrapolated from 0.8B measurements)

No web or Context7 lookups were needed: every question concerned in-repo code, contracts and local data.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all components in-repo and present on the host.
- Architecture (waiver, door, slice): HIGH — refusal sites and data structures read directly.
- SetFit arm: MEDIUM — blockers verified; memory at 100 rows interpolated.
- Kev arm: LOW — never run at 4B in Rust; disk blocking; cost extrapolated.
- Pitfalls: HIGH — each tied to a read line or a reproduced failure.

**Research date:** 2026-10-26
**Valid until:** 2026-11-09 (fast-moving: uncommitted justfile region work, partner client changes)
