# Phase 12: motivation-v1 decide server for experiment-coach (pilot, augury-dev) - Context

**Gathered:** 2026-10-25 (interactive discuss-phase; the four owner decisions of 2026-10-26 recorded in ROADMAP.md are carried in as D-01..D-04)
**Status:** Ready for planning

<domain>
## Phase Boundary

Ship a PILOT `motivation-v1` classifier of a post's motivational function, labels
`[social_affiliative, conspiratorial, other]` in that order, as its own MCP server
`aprender-mcp-decide-motivation` on pmcp.run (10,240 MB tier, augury-dev), the way antisemitism-v1 was shipped in
Phase 10. The served model is a Laya fine-tune. On the SAME stratified held-out split, SetFit and Kev-4B few-shot
are measured as reference arms, so the method choice rests on a measurement. The request is
`docs/requests/aprender.md` in motivation-experiment (commit 0c46b68); the data is
`datasets/motivation/Motivation Dataset - Sheet1.csv` (395 rows: 89 Social-affiliative / 55 Conspiratorial
explanation / 251 None; every post already classified antisemitic, so this is a second-stage model) and the guide is
Dr. Iris Yuster's 25 October 2026 draft (`Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx`).

In scope: data preparation, two measured task.json variants and the owner's choice, the folded calibration-slice ×
temperature-cap spike, the three-seed Laya gate run, the SetFit and Kev-4B reference cells, pack and verify (with
the declared pilot waiver if needed), the new thin server crate, the deploy, the acceptance replay and the hand-off
record.

Not in this phase: server-side prior correction; per-source results; evidence-span / audit output; landing the Kev
spike branch; serving SetFit or Kev; any change inside motivation-experiment.

</domain>

<decisions>
## Implementation Decisions

### Owner decisions already made (2026-10-26, from ROADMAP.md, recorded as given)
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

### Bake-off rules
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

### Guide → task.json
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

### Split and metrics on 395 rows
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

### Calibration and gate failure
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

### Amendments after research (owner, 2026-10-26, at plan-phase)
- **D-23 (deploy target):** deploy `aprender-mcp-decide-motivation` with profile `ze-kasher-dev`, region us-east-1,
  the same account, weights bucket and endpoint host pattern as `aprender-mcp-decide-antisemite`. "augury-dev" in
  the roadmap title is not an AWS target for this phase. No region work is needed. The uncommitted `region`
  parameter in the justfile is not this phase's work, so plans must not depend on it or commit it.
- **D-24 (Kev arm, amends D-07/D-11):** Kev-4B goes ahead. Disk is freed by `cargo clean` on the local
  checkout(s); the main `target/` is about 19 GB, giving about 44 GB free against the about 34 GB needed for the
  merge and GGUF conversion. The plan still times one row on the 4B model, and stops at a checkpoint with the
  projected total before the full arm runs. The Kev prompt is guide Sections 4–6 WITHOUT §4.5 (6,811 Qwen tokens,
  under Kev's 8,192 row limit), and the evidence records the omission. If the 4B Rust path cannot be made to
  run, the arm is recorded as blocked with a failure class, as SetFit was in Phase 10, and nothing is faked.
- **D-25 (SetFit arm, amends D-08):** use the s32 cell (the largest legal one; the conspiratorial pool is about
  36, and the allowed shots are {8,16,32,64}), recorded as a declared recipe deviation. Extend
  `laya_dir_to_canonical` to K labels, keeping Phase 10's binary output byte-identical. Pull in the deferred plan
  10-06 (`apr eval --dump-probabilities`) so SetFit's macro-F1, false-assignment rate and tertiles come from the
  same metrics code as the other arms (OPS-03).
- **D-26 (variant (b) door, amends D-09):** if the owner picks a task variant with P > 100, the door becomes
  per-artifact, `min(8, floor(800 / P))`, never below 4, through a `decide-tool-boundary-v1` change.
  antisemitism-v1 keeps 8. experiment-coach (commit 52f31d7) splits a batch only on a token refusal and sends
  batches of 4, so a door below 4 would break the partner. This contract work happens only if variant (b) is
  chosen at the D-09 checkpoint.
- **Research corrections recorded:** the eval fraction is a constant in the prepare script (EVAL_DIVISOR), not a
  contract value, so the D-13 contract-amendment branch never fires. `calibration_temp_max` lives in
  `laya-finetune-gate-v1.yaml` and `laya/temperature.rs`, not in `laya-parity-v1` (D-18 applies to those files).
  The calibration-slice fraction is a global contract minimum enforced by the Rust verifier, so changing it
  globally would fail re-verification of the live antisemitism-v1 artifact; any change must be per-run. The
  folded spike must not read `eval.jsonl` labels and runs on a hold-out carved from train.

### Carried forward from Phase 10 (unchanged)
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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The request, data and guide (motivation-experiment, outside this repo)
- `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md` — the request (commit 0c46b68): contract, dataset notes, cautions, and the tests they will run
- `/Users/guy/projects/mcp/motivation-experiment/datasets/motivation/Motivation Dataset - Sheet1.csv` — 395 rows, columns `Post (English, original)`, `Class`
- `/Users/guy/projects/mcp/motivation-experiment/datasets/motivation/Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx` — Sections 4–6 (definitions, Tables 2–5, decision rules), 7 (Table 6 worked examples), 9 (model instruction), 10.3 (example set = dataset's function rows), 10.2 Table 7 (field prior), 11 (open issues)
- `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md` — sections 4, 5.4, 7 (no breaking change during Phases 2–3)

### Phase 10, which this phase repeats
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-CONTEXT.md` — D-01..D-20 and their amendments
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-LEARNINGS.md` — lessons, patterns, surprises (verify timings, login window, F-10-12-1/2, register growth)
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-HANDOFF.md` — the hand-off shape to follow
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-01-SUMMARY.md`, `10-TASK-EVIDENCE.json` — the two-variant task.json measurement and owner checkpoint
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-03-SUMMARY.md`, `10-04-SUMMARY.md` — probe and gate run
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-05-SUMMARY.md`, `10-SETFIT-CELL-EVIDENCE.json` — the SetFit cell and the `apr eval` memory finding
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-12-PLAN.md`, `10-RENAME-DEPLOY-EVIDENCE.json` — per-model thin crate, new server id deploy, F-10-12-1/2/3

### Back office, contracts, crates
- `scripts/laya_train/prepare_goldstandard.py`, `train.py`, `data.py`, `contract.py`, `README.md` — the analog to copy
- `contracts/laya-finetune-gate-v1.yaml` — gate thresholds, seed policy, eval_set claim (waiver clause lands here or in a sibling)
- `contracts/laya-parity-v1.yaml` — `calibration_temp_max`, re-score bars (A1)
- `contracts/decide-apr-v1.yaml` — task_json_schema (criteria document order = label index)
- `contracts/decide-tool-boundary-v1.yaml` — door (classify_max_texts 8, 800 built tokens)
- `crates/aprender-decide/` (`src/task.rs`, `src/artifact.rs`, pack and `examples/pack_laya.rs`) — where a declared waiver must be honoured
- `crates/aprender-mcp-decide-lambda/` (library + README), `crates/aprender-mcp-decide-antisemite-lambda/` — the thin-crate template
- `crates/aprender-core/tests/monorepo_invariants.rs` — FALSIFY-MONO-011 deployment-unit register
- `crates/aprender-mcp-setfit/`, `scripts/laya_train/setfit_cell_e1b16_cpu.json` — SetFit cell
- `justfile` — `laya-build-bootstrap`, `laya-deploy-config`, `laya-deploy`, `laya-deploy-verify`, `laya-deploy-selftest`

### Kev arm
- `../aprender-016-upstream-sync` (branch `spike/016-upstream-sync`) — the Rust Kev path, used in place
- `.planning/spikes/` (spikes 015–020) — Kev evaluation, GGUF handoff, pointer head, batched prefill

### Folded todo
- `.planning/todos/pending/spike-laya-calibration-slice-and-temperature-cap.md`

### Skills and memory
- `Skill("spike-findings-aprender")`: `references/laya-decision-model.md`, `references/laya-calibration-gate-and-pack-tolerance.md`, `references/aws-mcp-model-hosting.md`, and the Kev / LLM decision classifier references
- Project memory: Kev decision-model spikes (branch location); SetFit calibration is CPU-only; tiny-fixture bit-identity is not real-weight identity; Phase 4 verification status breaks several gsd-tools verbs

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/laya_train/prepare_goldstandard.py`: copy for `prepare_motivation.py`, changing the pins, the 3-label map, the stratification and the task text. It includes the planted-sentinel selftest proving no text is printed.
- `crates/aprender-mcp-decide-antisemite-lambda/`: template for `aprender-mcp-decide-motivation-lambda` (Cargo.toml, shim main.rs, README) on the shared `aprender-mcp-decide-lambda` library.
- The `--measure` path from 10-01: reports the largest door a task admits; reuse it for the two variants.
- The Phase 10 SetFit cell recipe and the `laya_dir_to_canonical` adapter.

### Established Patterns
- Gate decisions and waivers are declared in contracts BEFORE the run reads results. A refusal is a result, not a prompt to tune.
- Long runs are detached, with the exit status read from an rc file and the device proven from a `DEVICE ... used=` trace line.
- Access is checked read-only before live steps (`aws sts get-caller-identity`, `expires_at` only); the executor never refreshes credentials.
- Plans need a verify branch for each honest failure they can hit (gate-failed-waived, margin halt, spike says the clamp binds, `apr eval` OOM).

### Integration Points
- `aprender_decide::pack` gate re-derivation: the declared waiver is new behaviour here and needs a falsification test.
- FALSIFY-MONO-011 register plus `duplicate_bin_names_allowlist.txt`: grow both by one (D-02).
- The just recipes take the server id `aprender-mcp-decide-motivation`.

</code_context>

<specifics>
## Specific Ideas

- The field metric is the false-assignment rate. A false assignment means a matched message sent to a real
  account, so the hand-off leads with it, per arm and per length band.
- Three labels and a longer instruction raise the per-call prefix, so fewer texts fit per call. experiment-coach
  sends batches of 4 and retries nothing on a refusal, so the hand-off must state the measured door for the chosen
  variant.
- The guide's own output is a score per class with no class assigned below a threshold. experiment-coach applies
  `motivation_min_prob` (0.6 today) client-side. Report false-assignment rate and recall at 0.6 and 0.85 so they
  can align it.
- `other` in this file mixes no-evidence, weak-evidence and mixed-evidence posts (guide 10.3). The hand-off notes
  this when stating the prior.

</specifics>

<deferred>
## Deferred Ideas

- Server-side prior correction: the post-freeze retrain (D-03).
- Per-source-batch results: when the owner supplies the mapping (D-04, D-15).
- Evidence-span and audit output from guide Section 9 (D-12): a new tool or field, its own phase.
- A hard false-assignment ceiling as a gate clause: the post-freeze retrain (D-05).
- Serving SetFit or Kev if either beats Laya: a later phase (D-06).
- Landing `spike/016-upstream-sync` (upstream merge, MTP `nextn_predict_layers` fix, batched prefill): separate work (D-07).
- Thread context (parent plus up to three preceding posts), per guide Section 11: the development data has none and `classify` sends the target only.
- Accuracy on posts not viewed during development, as the guide requires: when the partner supplies them.
- Raising `calibration_temp_max` beyond 5.0: only if the folded spike shows it binding, and then as an owner architectural decision (D-18).

### Reviewed Todos (not folded)
- `aprender-train-gpu-ledger-tests-red.md`, `qwen35-9b-hybrid-forward-unimplemented.md`, `workspace-test-gate-blocked-by-renacer-validate.md`: keyword matches only, unrelated to this phase.

</deferred>

---

*Phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot*
*Context gathered: 2026-10-25*
