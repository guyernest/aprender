---
phase: 12
reviewers: [codex]
reviewer_instances: [fable]
reviewed_at: 2026-10-26T00:00:00Z
plans_reviewed: [12-01-PLAN.md, 12-02-PLAN.md, 12-03-PLAN.md, 12-04-PLAN.md, 12-05-PLAN.md, 12-06-PLAN.md, 12-07-PLAN.md, 12-08-PLAN.md, 12-09-PLAN.md, 12-10-PLAN.md, 12-11-PLAN.md, 12-12-PLAN.md, 12-13-PLAN.md, 12-14-PLAN.md, 12-15-PLAN.md, 12-16-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review — Phase 12

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

Both reviewers ran on the plan set at commit `47f7a065a` with read access to the repo, and both covered all 16 plan ids.
The codex lane's stderr shows no workflow or skill invocation; the fable lane returned a full, source-cited review (no credits refusal this time).

## Codex Review

## 12-01

**Summary.** The preparation and approval sequence is sound, with one privacy gap in malformed-input handling.

**Strengths**

- Reusing `measure_task` checks actual head and option retention through the existing builder, rather than estimating prefix length (`scripts/laya_train/prepare_goldstandard.py:263`).
- The proposed split validation matches the trainer’s overlap and calibration checks (`scripts/laya_train/train.py:736`, `scripts/laya_train/data.py:283`).

**Concerns**

- **MEDIUM:** Echoing an unknown `Class` value when it is at most 30 characters contradicts the no-text guarantee. A misplaced post fragment is still caller content, regardless of which column contains it. The existing reader also echoes short invalid labels, so copying that pattern does not establish privacy (`scripts/laya_train/prepare_goldstandard.py:150`).
- **LOW:** The new reader specification omits the existing explicit handling of invalid UTF-8 and CSV parser errors (`scripts/laya_train/prepare_goldstandard.py:157`).

**Suggestions**

- Report only row number, field name and character count for invalid labels.
- Add malformed-encoding and parser-error cases, including a sentinel placed in the `Class` field.

**Risk Assessment: MEDIUM.** The core preparation path is well supported; refusal-path privacy needs tightening.

## 12-02

**Summary.** The waiver decision is appropriately placed, but the prescribed fixture modification conflicts with the new rules.

**Strengths**

- Deciding the waiver after re-scoring and `check_gate` preserves the existing eligibility authority (`crates/aprender-decide/src/verify.rs:3233`, `crates/aprender-decide/src/verify.rs:3266`).
- Keeping the seed-policy check afterward preserves the existing refusal order (`crates/aprender-decide/src/verify.rs:3283`).

**Concerns**

- **HIGH:** Adding a waiver claim to `fullest_copy` while keeping its permissive policy breaks the existing binding sweep’s positive control. That sweep explicitly requires the fixture to verify successfully (`crates/aprender-decide/src/verify/tests.rs:2474`). Under the proposed rules, its passing gate plus claim produces `WaiverWithoutFailure`; making its gate fail instead requires a matching declaration and changes other sweep assumptions.
- **MEDIUM:** Declaration IDs need uniqueness validation. Otherwise duplicate IDs introduce ambiguous matching between the Rust and Python readers. Contract views intentionally ignore unknown policy fields, so explicit validation matters (`crates/aprender-decide/src/verify.rs:205`).

**Suggestions**

- Preserve the passing fixture and add a separate, valid ECE-failed waived fixture. Sweep both, with their corresponding policies.
- Reject duplicate declaration IDs and malformed identity hashes.

**Risk Assessment: HIGH.** The specified fixture change prevents the required full suite from passing.

## 12-03

**Summary.** The artifact compatibility design is strong, but the binding-test adaptation is incomplete.

**Strengths**

- Omitting an absent manifest field is essential: `verify_path` requires an existing artifact to equal a fresh pack of its run (`crates/aprender-decide/src/verify.rs:3413`).
- Testing manifest strip/add/change cases exercises the existing source-binding boundary (`crates/aprender-decide/src/artifact.rs:1603`).

**Concerns**

- **HIGH:** Taking the union of manifest leaves does not finish updating `manifest_bindings_table_matches_manifest_leaves`. Its second half resolves every source pointer against the **unwaived** `packed()` report; `report:/pilot_waiver/id` will still be absent (`crates/aprender-decide/src/artifact/ladder.rs:1307`, `crates/aprender-decide/src/artifact/ladder.rs:1324`).
- **MEDIUM:** The generic mutation sweep also uses only `packed()`, leaving the optional waiver binding outside that sweep (`crates/aprender-decide/src/artifact/ladder.rs:1232`).
- **MEDIUM:** Updating lifecycle schema checking is required, rather than conditional: it currently compares the exact report key set without `has_waiver` (`scripts/laya_train/lifecycle.py:498`).

**Suggestions**

- Resolve bindings against each applicable fixture and run mutations on both.
- Include `lifecycle.py` explicitly and add an integration case that actually produces a waived report.

**Risk Assessment: HIGH.** Compatibility is carefully considered, but mandatory verification would fail as specified.

## 12-04

**Summary.** The thin-crate approach fits the existing architecture, and the health retry addresses a verified deployment failure.

**Strengths**

- The existing shim test discovers matching crate directories automatically and checks their dependency surface (`crates/aprender-mcp-decide-lambda/src/tests.rs:623`).
- The current deployment performs exactly one health GET before containment, confirming the retry’s integration point (`justfile:2068`).

**Concerns**

- **MEDIUM:** The stated ten-minute bound excludes request time. Forty attempts with a 30-second curl timeout and 39 fifteen-second sleeps can take approximately 29.75 minutes. That matters to the subsequent deployment and identity sequence (`justfile:2068`, `justfile:2078`).
- **LOW:** The behavior table mentions immediate refusal of 503, but the prescribed offline cases explicitly exercise only 500.

**Suggestions**

- Define a maximum elapsed duration, or document the full worst-case bound including HTTP timeouts.
- Add 503 and invalid retry-configuration cases to the offline table.

**Risk Assessment: MEDIUM.** The change is localized, but its operational timeout needs an accurate bound.

## 12-05

**Summary.** Centralizing metrics is appropriate, but the comparison needs stronger input identity and statistically valid interval checks.

**Strengths**

- Reusing the existing exactly summed metrics preserves agreement with Rust (`scripts/laya_train/metrics.py:62`, `scripts/laya_train/metrics.py:79`).
- Explicitly declaring ranking and threshold rules before measurement limits retrospective selection.

**Concerns**

- **HIGH:** A percentile bootstrap interval need not contain the observed estimate. Using the existing ECE implementation, I reproduced an observed ECE of `0.0` with the prescribed stratified bootstrap yielding approximately `[0.006, 0.098]`. ECE’s absolute bin errors explain this behavior (`scripts/laya_train/metrics.py:89`). The final acceptance check incorrectly rejects such valid results.
- **HIGH:** SetFit truth-label agreement cannot establish row identity. Swapping two same-label rows preserves truth and overall accuracy while corrupting length-band metrics. The adapter creates distinct row IDs that the proposed dump discards (`crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs:304`).
- **MEDIUM:** Finiteness alone does not validate probabilities; `_as_arrays` does not enforce unit range or row sums (`scripts/laya_train/metrics.py:40`).

**Suggestions**

- Validate interval ordering and bounds without requiring point containment.
- Bind SetFit rows to IDs/hashes and validate normalized probability distributions.

**Risk Assessment: HIGH.** Otherwise plausible reports can be rejected or silently misaligned.

## 12-06

**Summary.** The adapter generalization and same-pass dump fit the current pipeline, but the dump lacks sufficient provenance for downstream alignment.

**Strengths**

- The adapter already preserves evaluation-file order when constructing test items (`crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs:442`).
- Collecting probabilities inside `evaluate_test` preserves the existing lock, token and grant chain (`crates/apr-cli/src/commands/eval/setfit.rs:415`, `crates/apr-cli/src/commands/eval/setfit.rs:524`).

**Concerns**

- **HIGH:** The six-key dump identifies the artifact but not the evaluated dataset or individual rows. The existing evaluation report already carries dataset fingerprints, demonstrating that artifact identity alone is insufficient (`crates/apr-cli/src/commands/eval/setfit.rs:462`).
- **MEDIUM:** Tests and adapter builds do not necessarily produce a fresh `apr` executable. `apr_bin.sh` resolves and checks binaries; it deliberately does not build one (`scripts/apr_bin.sh:93`, `scripts/apr_bin.sh:491`).

**Suggestions**

- Add row identities and dataset provenance, either to a versioned dump or a bound sidecar.
- Explicitly build the committed CLI with SetFit features before the tracer and later cell runs.

**Risk Assessment: HIGH.** The functionality is feasible, but insufficient provenance undermines the comparison it enables.

## 12-07

**Summary.** The train-only spike design protects the gate split, but failure handling is not executable for several declared outcomes.

**Strengths**

- Reusing `calibration_split` preserves existing grouping and stratification semantics (`scripts/laya_train/data.py:283`).
- The reusable spike already retains calibration logits and computes bounded and oracle temperatures (`.claude/skills/spike-findings-aprender/sources/027-laya-calibration-slice-and-tcap/run_cell.py:95`).

**Concerns**

- **HIGH:** A probe OOM is supposed to jump to Task 3, but Task 3 unconditionally opens and hashes `12-SPIKE-EVIDENCE.json`. That file does not exist if the probe failed before the spike. The checkpoint verifier therefore crashes instead of accepting an OOM decision (`.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-07-PLAN.md:307`).
- **HIGH:** Failed cells are described as reportable outcomes, while aggregation requires nine complete metric rows and a zero grid exit status.
- **MEDIUM:** Cached results are skipped by existence, without a required binding to data, epochs and harness identity. The copied harness has the same existence-based cache behavior (`.claude/skills/spike-findings-aprender/sources/027-laya-calibration-slice-and-tcap/run_cell.py:185`).

**Suggestions**

- Define separate probe-failed, grid-incomplete and completed states with nullable evidence.
- Validate cache identities and stop before aggregation when cells are incomplete.

**Risk Assessment: HIGH.** The normal path is clear; the promised failure paths need redesign.

## 12-08

**Summary.** A conditional, dataset-bound override is the correct response to the global-policy hazard.

**Strengths**

- The current verifier enforces the calibration fraction as a minimum, confirming why changing the global value affects existing runs (`crates/aprender-decide/src/verify.rs:1715`).
- Input identities are checked before slice validation, providing an appropriate verified hash for override lookup (`crates/aprender-decide/src/verify.rs:3178`).

**Concerns**

- **MEDIUM:** Duplicate entries for one train hash have no defined resolution rule. Independently implemented Python and Rust lookup could select different fractions.
- **MEDIUM:** The specified lookup tests do not prove that `cheap_checks` and the real trainer actually use the override. The current verifier passes the global policy field directly (`crates/aprender-decide/src/verify.rs:3184`).
- **LOW:** The new recipe-ID builder does not include calibration policy, so reproducibility also depends on recording the applicable contract revision (`scripts/laya_train/contract.py:285`).

**Suggestions**

- Reject duplicate hashes and non-finite fractions in both readers.
- Add an integration fixture that fails under the override minimum, plus a trainer split-count check.
- Record the effective policy revision with the run evidence.

**Risk Assessment: MEDIUM.** The architecture is appropriate; validation must cover its actual wiring.

## 12-09

**Summary.** The per-artifact door is reasonable, but its measurement and enforcement need a more explicit integration design.

**Strengths**

- The current builder demonstrably rejects a task when its minimum rows exceed the total budget (`crates/aprender-mcp-decide/src/lib.rs:751`).
- `build_server_with_limits` already passes one limits object to both the description and service (`crates/aprender-mcp-decide/src/lib.rs:792`).

**Concerns**

- **HIGH:** The proposed probe derivation is circular with the existing helper. `served_min_row_tokens` calls `check_served_task_fits`, which requires limits and rejects a P > 100 task under the existing eight-text door before the new door can be derived (`crates/aprender-mcp-decide-lambda/src/probe.rs:1033`).
- **MEDIUM:** Other public entry points retain fixed limits: `ClassifyService::served` uses `CONTRACTED`, and `parse_args` performs its count-first check against that constant (`crates/aprender-mcp-decide/src/lib.rs:589`, `crates/aprender-mcp-decide/src/lib.rs:727`).
- **LOW:** `served_for(0)` needs explicit rejection before division.

**Suggestions**

- Separate minimum-row measurement from door validation.
- Thread derived limits through every serving and parsing entry point.
- Test real P > 100 probe construction and malformed requests above the derived count.

**Risk Assessment: HIGH.** Variant (b) can remain unservable if the helper dependency is implemented literally.

## 12-10

**Summary.** The rehearsal and fixed s32 recipe provide a defensible reference measurement, with operational gaps around binary freshness and rehearsal interpretation.

**Strengths**

- The existing recipe already separates selection, training, validation locking and test evaluation (`justfile:1366`, `justfile:1381`).
- The test evaluator computes accuracy in the same classification loop where probabilities will be collected (`crates/apr-cli/src/commands/eval/setfit.rs:524`).

**Concerns**

- **MEDIUM:** A fresh executable is a prerequisite, but sourcing `apr_bin.sh` does not rebuild it. Commits since 12-06 can make the existing executable fail freshness checks (`scripts/apr_bin.sh:491`).
- **MEDIUM:** The s8 rehearsal evaluates the full test split before the s32 measurement. This is acceptable for memory measurement only if its quality results are explicitly excluded from recipe decisions; the current recipe prints accuracy (`justfile:1388`).
- **LOW:** The memory rule is retrospective: it cannot prevent an unexpectedly excessive rehearsal from exhausting the host.

**Suggestions**

- Build and pin the CLI before starting.
- Record that rehearsal quality is excluded from selection.
- Add a practical resource stop condition for the rehearsal.

**Risk Assessment: MEDIUM.** The arm is reproducible once those operational prerequisites are explicit.

## 12-11

**Summary.** The declaration-first gate sequence is strong, but the rerun state machine does not enforce its own limits.

**Strengths**

- Training already binds input hashes before fitting and checks them again before writing the report (`scripts/laya_train/train.py:744`, `scripts/laya_train/train.py:910`).
- The Rust packer writes the artifact only after verification succeeds (`crates/aprender-decide/src/verify.rs:3404`).

**Concerns**

- **HIGH:** After a failed second run, Task 3 still offers `rerun-once` and rewrites to the same `-run2` directory. There is no guard enforcing the promised maximum of two attempts; the trainer then refuses the populated directory (`scripts/laya_train/train.py:762`).
- **MEDIUM:** After a successful rerun, the existing owner reply records a non-skipped rerun decision, while checkpoint verification expects every eligible outcome to have a skipped reply. The transition needs distinct approval and completion records (`.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-11-PLAN.md:297`).
- **MEDIUM:** An identical training rerun cannot repair deterministic declaration, packing or door defects, although those failures share the same options.

**Suggestions**

- Use immutable attempt records and explicitly cap attempts at two.
- Preserve rerun authorization separately from final checkpoint disposition.
- Separate training failures from implementation defects requiring repair.

**Risk Assessment: HIGH.** Failure and recovery paths can dead-end despite valid owner decisions.

## 12-12

**Summary.** Deployment has strong identity checks, but it cannot deploy the explicitly supported successful rerun.

**Strengths**

- The deployment recipe re-verifies the exact artifact before AWS deployment (`justfile:1976`).
- Compile-log checks and scoped-role verification reduce wrong-binary and wrong-permission risks (`justfile:2051`, `justfile:2058`).

**Concerns**

- **HIGH:** Both upload and deployment hard-code `models/decide/laya-motivation-v1` as the run directory. If 12-11 ships `-run2`, verification compares that artifact with the first run and refuses on report identity or fresh-pack equality (`crates/aprender-decide/src/verify.rs:3443`). The hard-coded upload appears at `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-12-PLAN.md:116`.
- **MEDIUM:** A recorded “45 minutes left” line is historical evidence, not a current token-validity check after an interruption or resumed session.

**Suggestions**

- Read the selected run directory from gate evidence for every upload, deploy and recovery command.
- Recheck `expires_at` immediately before the live operation, including resumed attempts.
- Add an offline deployment test using a successful second-run artifact.

**Risk Assessment: HIGH.** The approved rerun path currently cannot reach deployment.

## 12-13

**Summary.** The acceptance replay is well grounded, but failed timing measurements can prevent the intended owner checkpoint.

**Strengths**

- Cold-start proof uses a function configuration change and a matching CloudWatch load line (`justfile:2842`, `justfile:2859`).
- Replay validation already checks the response against expected labels and identity (`crates/aprender-mcp-decide-lambda/src/probe.rs:809`).

**Concerns**

- **HIGH:** Failed HTTP probes produce sample records with nullable `probe_id` and `elapsed_ms` (`justfile:2901`). The plan’s unconditional string concatenation and maximum calculations cannot normalize those records. Earlier AWS failures can also exit before any evidence file is written (`justfile:2845`).
- **MEDIUM:** The batch-of-four verifier accepts any truthy `line` without requiring its replay checks to pass (`.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-13-PLAN.md:233`).
- **LOW:** The threat-model statement that probes do not alter either function conflicts with the deliberate cold-start configuration updates.

**Suggestions**

- Normalize failed samples with explicit nullable fields and retain every error.
- Derive timing summaries only from measured values.
- Require all response checks on a successful four-text call and record the deliberate configuration changes.

**Risk Assessment: HIGH.** The failure handling is weakest precisely when the live endpoint needs an owner decision.

## 12-14

**Summary.** Proving the 4B path before the full run is sensible, but the prescribed build preparation contains a deterministic blocker.

**Strengths**

- The existing driver directly exercises `prefill_hidden` and compares probabilities with the torch oracle (`.planning/spikes/020-qwen35-batched-prefill/src/main.rs:64`).
- Strict Kev encoding checks the combined state and branch against the row limit (`.planning/spikes/015-kev-vs-setfit-few-shot/vendor/kev/kev/model.py:95`).

**Concerns**

- **HIGH:** Copying the spike-020 lockfile, renaming the package, then building with `--locked` requires a lockfile update. The copied lock still names `qwen35-batched-prefill` (`.planning/spikes/020-qwen35-batched-prefill/Cargo.lock:991`). This can manufacture a “blocked/build” result before testing feasibility.
- **MEDIUM:** The copied exporter holds the loaded Kev model while loading another complete causal model, increasing peak memory substantially (`.planning/spikes/017-kev-rust-forward-parity/tools/export_and_oracle.py:27`, `:32`).
- **MEDIUM:** Short public-row parity does not cover the approximately 6.8k-token operating regime.

**Suggestions**

- Regenerate the renamed spike’s lockfile offline, then build locked.
- Reduce duplicate model residency.
- Add one long synthetic parity case before the budget decision.

**Risk Assessment: HIGH.** Build correctness and realistic memory/context verification need attention.

## 12-15

**Summary.** Train-only fitting is correctly separated from evaluation, but the numerical and resume behavior needs explicit implementation requirements.

**Strengths**

- The existing split helper can keep bias fitting and temperature fitting disjoint (`scripts/laya_train/data.py:303`).
- Reusing `eval_probs_obj` provides the required row hashes and probability serialization (`scripts/laya_train/train.py:490`).

**Concerns**

- **HIGH:** Resuming solely by `(split, row)` does not bind existing scores to model, head, prompt, token IDs or driver revision. Stale output can silently satisfy row-count checks. A crash can also leave a partial final JSON line; recovery behavior is unspecified.
- **MEDIUM:** Taking `log` of f32 probabilities can produce infinities after softmax underflow. `fit_temperature` explicitly rejects non-finite logits (`scripts/laya_train/gate.py:71`).
- **MEDIUM:** `fit_temperature` returns three values, not a scalar T (`scripts/laya_train/gate.py:65`, `:91`). Also, importing `fewshot.py` executes its top-level argument parser and evaluation work (`.planning/spikes/015-kev-vs-setfit-few-shot/tools/fewshot.py:75`).

**Suggestions**

- Bind resumable output to immutable input hashes and define partial-line recovery.
- Use dumped raw logits with the head temperature; unpack the temperature result.
- Extract or copy the pure bias-fitting function without importing the executable script.

**Risk Assessment: HIGH.** Long-run recovery could otherwise produce a plausible but invalid reference result.

## 12-16

**Summary.** The hand-off covers the owner’s disclosures thoroughly, but final verification inherits an invalid statistical condition and remains too textual for its numeric claims.

**Strengths**

- Exact reproduction of Laya metrics is supported by the shared metric implementation and f32 probability writer (`scripts/laya_train/metrics.py:112`, `scripts/laya_train/train.py:490`).
- Reporting blocked arms and keeping Laya served follows the phase’s explicit decisions.

**Concerns**

- **HIGH:** Final verification requires every percentile interval to contain its point estimate. The counterexample described under 12-05 demonstrates that this can reject correct output. The condition is explicit at `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-16-PLAN.md:129`.
- **MEDIUM:** Keyword-presence checks do not verify that hand-off numbers, priors, denominators and gate disclosures match their records.
- **MEDIUM:** D-05’s strict `>` reporting differs at exact threshold equality from the partner’s `>=` treatment rule (`/Users/guy/projects/mcp/motivation-experiment/crates/core/src/treatment.rs:59`). That distinction should be disclosed rather than presenting the rates as exact client replay.

**Suggestions**

- Remove point-containment acceptance.
- Generate numeric tables from evidence and validate quoted values structurally.
- Preserve the declared metric while documenting threshold equality and separately reporting client-equivalent counts if useful.

**Risk Assessment: HIGH.** The final artifact can be blocked by valid statistics or convey unchecked numeric claims.

## Overall assessment

**Coverage: 16/16 plans reviewed against repository source. Changes requested before execution.**

The architecture largely achieves the owner’s amended goal: a Laya pilot with measured reference arms and explicit limitations. The main blockers are fixture consistency, optional-field binding tests, bootstrap acceptance, failure-state handling, the derived-door helper dependency, rerun deployment paths, and the renamed Kev lockfile.

Resolve those before expensive training or live deployment. Then verify the recovery paths with synthetic fixtures, including a successful second run and failed cold-start samples.

This was a read-only review. No workflows, builds, external AI CLIs or file mutations were performed. The ECE counterexample was reproduced entirely in memory.

---

## Claude Review (fable)

# Plan Review — Phase 12 (16 plans)

**Method.** Every mechanism claim below was checked against the working tree at `gsd/phase-2-contract-gate` on 2026-10-26: the Rust verifier and artifact binder, the three contracts, the register and allowlist, the justfile recipes, the Python back office, the Phase 10 records, and the host state (data dirs, worktree, disk, vendors, partner files). Where a plan's claim held, I say so with the line; where it did not, the concern cites the line that contradicts it.

**Headline.** The plan set is unusually well grounded: nearly every cited line number is right, the declare-before-read spine is enforced by verifies that re-derive outcomes from recorded runs, and every honest failure branch (waived ECE, margin halt, blocked SetFit, blocked Kev, verify_failed deploy) has its own record shape. Three findings need attention before execution:

1. **HIGH, 12-02 Task 2:** adding a `pilot_waiver` claim to `fullest_copy` breaks `every_run_field_is_bound_or_report_only`, whose control asserts that copy verifies (`crates/aprender-decide/src/verify/tests.rs:2474-2477`). A passing run carrying a claim is exactly the new `WaiverWithoutFailure` refusal the same plan introduces.
2. **MEDIUM, 12-12:** Phase 10's executor was refused by the permission classifier on `laya-upload` and `laya-deploy` and the owner ran both (`10-12-SUMMARY.md:128`, `10-RENAME-DEPLOY-EVIDENCE.json:174,592`). Plan 12-12 assigns both to auto tasks with no owner-run branch in the evidence shape.
3. **MEDIUM, 12-09 branch (b):** `parse_args` checks the text count against the compiled `ClassifyLimits::CONTRACTED` before deserialization (`crates/aprender-mcp-decide/src/lib.rs:727-730`); a per-artifact door that only reaches `build_server` leaves two different count bounds in play.

---

## 12-01

**Summary.** Prepares the shared split and the two measured task variants, then stops for the owner. It copies `prepare_goldstandard.py` faithfully, reuses its measurement helpers rather than copying them, and binds the owner's approval to a sha prefix of the bytes shown. Every host precondition holds: the CSV hashes to `bc61c76bd29bcdd4…`, the partner HEAD is `0c46b68`, the Laya tokenizer snapshot is present, and `data/decide/motivation-v1` does not yet exist.

**Strengths.**
- The red `--selftest-tokenizer` claim is real: `scripts/laya_train/prepare_goldstandard.py:541-542` asserts `stance["P"] == 57 == contract_constant("served_task_min_row_tokens")` and the contract constant is 86 (`contracts/decide-tool-boundary-v1.yaml:326`). Repairing it to a literal is correct.
- Importing `measure_task`, `door_fields`, `contract_constant`, `load_tokenizer`, `tweet_report` instead of `report()`/`read_rows` is right: `read_rows` is bound to the six-column antisemitism header (`prepare_goldstandard.py:95,145`).
- The counts the verify hardcodes (eval 23/14/63, train 66/41/188, slice 75) follow from `ceil(n/4)` and `data.calibration_split`'s `max(2, ceil(0.25 n))` (`data.py:290`); I recomputed them.
- The OWN-HUNKS COMMIT procedure is necessary: `git diff --stat -- justfile` shows 18 insertions and 7 deletions of the owner's region work in the tree today.
- The sentinel selftest pattern (both streams, control proving capture) is exactly `prepare_goldstandard.py:466-491`.

**Concerns.**
- **LOW. Key-name mismatch.** The plan's measure output and verify expect `post_tokens` (`d["post_tokens"]["n"] == 395`) but `tweet_report` returns the key `tweet_tokens` (`prepare_goldstandard.py:296`). The executor must rename the key when assembling the line; the plan does not say so.
- **LOW. `prefix-door` refusal is unreachable.** `measure_task`'s `p_cap` is 196 for both 2- and 3-option shapes (research table; `p_cap` is the whole-prefix cap), so `P > 200` cannot occur and the refusal is dead code. Harmless, but the verify's `100 < b["P"] <= 196` is the real bound.
- **LOW. Gates sweep row targets the wrong script.** The verify runs `LAYA_GATES_ONLY=prefix-budget`, whose row targets `just laya-prepare-goldstandard` (`scripts/laya_gates.tsv:32`). That only proves the drift check tolerates the new thin wrapper; it exercises nothing of `prepare_motivation.py`. Fine as a drift check, but the plan text reads as if it tests the new script.

**Suggestions.**
- State explicitly that the measure line maps `tweet_tokens` to `post_tokens`, or keep `tweet_tokens` and adjust the verify.
- Consider a `prefix-budget-motivation` gates row later (not required for this plan).

**Risk: LOW.** The analog is proven, the data facts are measured, and the one-way decision is gated by a blocking checkpoint bound to bytes.

---

## 12-02

**Summary.** Builds the D-19 waiver's refusal side first: a typed contract view, one policy mapping, a pure `decide_pilot_waiver`, two new refusals, `VerifyReport.waived`, and the falsification table. The refusal site it extends is exactly where the plan says: `check_gate` at `verify.rs:3266`, `GateFailed` at `:3268`, seed-policy check at `:3285`; `GateContractView` is documented as not `deny_unknown_fields` at `:205`; `from_contract_views` is the one mapping at `:378`; `exit_code` at `:881`.

**Strengths.**
- `WavableClause` with only an `EcePost` variant makes a margin waiver un-deserializable: D-20 is enforced at contract load, before any run is read.
- `RunIdentity` built from already-verified values is sound: `PackInputs::from_run_dir` hash-checks `recipe_id` against `recipe.json` bytes and every `inputs_sha256` against the data files (`pack.rs:678,726-728`), so the waiver cannot bind to unverified hashes.
- `check_gate`'s `PassDisagrees` check (`verify.rs:2473-2487` per research) does not interfere: a waived run has `pass: false` in both the report and the recomputation.
- No serving path reads `gate.pass`: my grep over `aprender-mcp-decide*` and `aprender-decide/src/lib.rs` found only the rung-4 binding comparison (`artifact.rs:1656`) and `pack_laya`'s inspect print (`pack_laya.rs:236`). The D-19 premise holds.
- Deploy recipes read only `deploy_eligible` and `artifact_sha256` (`justfile:1987-1991, 2602-2606`), so adding `waived` to the JSON line is safe.

**Concerns.**
- **HIGH. `fullest_copy` + claim breaks an existing test.** Task 2 says to extend `fullest_copy` so its gate report carries a `pilot_waiver` block, "its policy stays permissive". But `every_run_field_is_bound_or_report_only` does `let control = base.verify(&policy).expect("the fullest copy verifies")` on `fullest_copy(&permissive_policy())` (`verify/tests.rs:2474-2477`), then mutates each leaf alone and classifies it bound or report-only (`:2521-2530`). With a claim on a passing fixture (the tiny fixture's own report has `pass: false, margin 0.0`, but `production_copy` under the permissive policy is what verifies today), the control becomes `WaiverWithoutFailure` and the test fails. For the new `/pilot_waiver/id` and `/pilot_waiver/waives/[*]` rows to be classified "bound", the base must be an **accepted waived run**: an induced ECE-only failure plus a declaration in the policy whose `applies_to` equals that copy's hashes. The plan's Task 1 tracer builds exactly such a declaration for one test; Task 2 must do the same for the shared fixture, or give the bindings sweep a separate waived copy and keep `fullest_copy` unchanged.
- **MEDIUM. A bad declaration edit disables all verification.** Because a `waives: [margin]` entry fails the whole `GateContractView` deserialization, a typo in `pilot_waivers` makes `laya-verify` of the antisemitism artifact fail too. That is the intended fail-closed, but the plan should say the symptom out loud so a future executor does not misread it as artifact drift.
- **LOW. `variant_name` and `Display` are exhaustive matches** (`verify.rs:891,921,1120`); two new variants need both arms. The plan names the variants; it does not name the Display arm.

**Suggestions.**
- Rewrite Task 2's `fullest_copy` instruction: make it an accepted waived run (induce `ece_post > 0.10` with consistent report values, set `pilot_waiver {id "test-pilot", waives ["ece_post"]}`, and extend `permissive_policy()` with a declaration whose `applies_to` is filled from the copy after it is built; or add `fullest_waived_copy()` and let `run_field_bindings_table_matches_fixture_leaves` take the union while `every_run_field_is_bound_or_report_only` iterates both bases).
- Add to the SUMMARY a one-line note that a malformed `pilot_waivers` block refuses every artifact at load.

**Risk: MEDIUM.** The design is right and the refusal sites are verified; the fixture interaction is a real defect in the plan text that would surface as an unexpected RED in an unrelated test.

---

## 12-03

**Summary.** Binds the waiver into artifact identity (`GateSummary.waiver`, rung-4 check, decide-apr-v1 bindings row), adds the trainer's claim through one `recipe_id_for`, and proves byte stability with one real `laya-verify` of the deployed antisemitism artifact. The sites are as cited: `GateSummary` is `deny_unknown_fields` at `artifact.rs:205-214`, filled at `:1213`; `check_manifest_bindings` holds a `[(&str, bool); 23]` table at `:1622` with `("gate.pass", …)` at `:1656`; decide-apr-v1 is 3.0.0 with `/gate/*` rows at `:239-251` and highest id FALSIFY-DECIDE-APR-013.

**Strengths.**
- `skip_serializing_if = "Option::is_none"` is mandatory and the plan says why: `verify_path` requires byte-identity with a fresh pack, and the deployed `2293bebb…` artifact exists on this host to prove it.
- `recipe_id_for` as the one builder is checked against the deployed run's `recipe.json` bytes and `gate-report.json` `recipe_id` (both present under `models/decide/laya-antisemitism-v1/`). The current bytes are `json.dumps(recipe, sort_keys=True, separators=(",", ":"))` at `train.py:774`.
- The AST comparison of `SeedRun`, `train_seed`, `calibration_logits` against the start commit makes the 12-07 coupling claim verifiable rather than asserted.
- `lifecycle.py:476,499` call `expected_keys` with four positional args; the `has_waiver=False` default keeps them valid.

**Concerns.**
- **MEDIUM. `every_manifest_leaf_is_bound` and a row without a leaf.** The plan unions leaves for `manifest_bindings_table_matches_manifest_leaves` (`ladder.rs:1289`), but `every_manifest_leaf_is_bound` (`ladder.rs:1232`) likely iterates the table and mutates each leaf on `packed()`; a `/gate/waiver` row whose leaf is absent from the unwaived pack needs an explicit branch (mutate on `packed_waived()` instead). The plan covers strip/add/change in a dedicated test but does not say how the generic leaf iterator treats the optional leaf.
- **LOW. No per-clause list exists in the trainer.** `gate.evaluate_gate` returns `{pass, thresholds, zero_shot, fine_tuned, margin}` only (`gate.py:222-240`). Task 2's "compute the failed clause names from the same quantities the decision used" is right but should name the two comparisons (`margin >= min_margin`, `ece_post <= max_ece`) so `matching_pilot_waiver`'s `failed` list is derived once, not re-derived differently in train.py and gate.py.
- **LOW. Real-weights cost.** Task 3's verify is about 21 minutes (`10-LEARNINGS.md:146`); plans 12-08 and 12-11 add more. Fine, but the host lock serializes them with 12-07's spike.

**Suggestions.**
- Add to Task 1: "`every_manifest_leaf_is_bound` iterates `/gate/waiver` on `packed_waived()`; every other row on `packed()`."
- Have `gate.py` expose `failed_clauses(report_or_metrics) -> list[str]` and use it from train.py.

**Risk: LOW-MEDIUM.** Mechanism verified; the generic-leaf-iterator interaction is the only open implementation question.

---

## 12-04

**Summary.** Offline deploy prep: the thin crate under D-02, register and allowlist growth, resolver proof and arm64 bootstrap, and the F-10-12-1 404-only retry with an offline shadowed-curl table. All register facts hold: `DEPLOYMENT_UNIT_BASELINE: usize = 9` at `monorepo_invariants.rs:445` with a `<=` assertion at `:447` and the "9 -> 10 needs a recorded CONTEXT decision" comment at `:443`; the `bootstrap` allowlist line at `duplicate_bin_names_allowlist.txt:16` lists four claimants; root `Cargo.toml:75` has the antisemite member; README says 92 workspace crates at line 54.

**Strengths.**
- The template is a true shim (`aprender-mcp-decide-antisemite-lambda/src/main.rs:7-12`) and `thin_server_crates_are_shims` (`aprender-mcp-decide-lambda/src/tests.rs:623`) discovers by directory, so no edit there is needed.
- The retry targets exactly the single `curl -fsS` GET at `justfile:2068`, and the 503-only identity retry pattern it copies is at `:2078-2088`. Using `-o body -w '%{http_code}'` instead of `-f` is the correct fix for telling 404 from 500.
- The drift check enumerates private recipes (`justfile:3126`), so the new `_laya-edge-health-get` with its `REFUSED` text needs a gates row; the plan adds `edge-health-get`.
- Reading N from the start commit and asserting N+1 handles the Phase 11 register-growth race honestly.

**Concerns.**
- **LOW. Bootstrap build time after `cargo clean`.** Not this plan's problem (12-14 cleans later), but `laya-build-bootstrap` cross-compiles the full dependency tree; budget it.
- **LOW. Retry bound.** 40 × 15 s = 10 min default is sized to one observation (about 8 min). Reasonable; the env override is the right escape.

**Suggestions.**
- In the selftest table, also assert that the written body file is absent or empty on the transport-failure case, so a stale `deploy-health-$SERVER.json` from a prior run cannot pass `_laya-edge-health-check`.

**Risk: LOW.** Every file the crate growth touches is enumerated and each has a test that catches omission.

---

## 12-05

**Summary.** Adds the bake-off metrics to `metrics.py` and a `bakeoff.py` whose ranking, tiebreak and "clearly beats" rules are declared in the docstring before any arm exists. `metrics.py` is as described: `_as_arrays` at 40, `_f1` at 54, `macro_f1` at 62, `ece_top_label` at 79, and the bit-identical numeric replay against `numeric_cases.json` (present, 317 KB).

**Strengths.**
- `ece_bootstrap_ci` calling `ece_top_label` keeps one ECE (OPS-03) and the class-stratified resample keeps the 14 conspiratorial rows at 14 per draw, which is the point of D-13's interval.
- Alignment by `exact_sha256` for Laya/Kev and by truth labels for the SetFit dump matches what each writer produces (`train.py:490 eval_probs_obj`; 10-06's six-key dump).
- Requiring Laya's numbers to reproduce the gate bit for bit is cheap insurance that the arm file is the shipped one: the gate numbers were computed from the same f32-widened probabilities.

**Concerns.**
- **LOW. Bootstrap cost.** 10,000 resamples × a Python 15-bin loop over 100 rows is seconds per arm; with three arms and three bands each, still under a minute. Acceptable, but the selftest should use a smaller B.
- **LOW. Tiebreak definition.** "Equal to 4 decimals" is declared, good; the per-band tables carry about 33 rows and 4-5 conspiratorial rows per band, so per-band precision/recall will be very noisy. The plan prints counts beside every rate, which is the right mitigation.

**Suggestions.**
- Emit the bootstrap interval for per-band ECE as well only if `n_band >= 30`; otherwise mark it `not_reported`, so the hand-off does not carry a 95 % interval on 11 rows.

**Risk: LOW.**

---

## 12-06

**Summary.** Generalises the SetFit adapter to K labels with identity arguments and lands plan 10-06 Task 1 (`--dump-probabilities`). The two-label lock is exactly at `laya_dir_to_canonical.rs:161-166`; the identity literals at `:459-461`; `CARVE_DOMAIN` at `:58` stays; the byte-identity control dir `data/decide/goldstandard-2024-setfit` exists.

**Strengths.**
- Byte-identity via `cmp` of all four Phase 10 output files is the right proof that K-generalisation changed nothing for antisemitism.
- 10-06 Task 1 is reused as the specification verbatim, including the clippy scope rule for apr-cli test targets that are red at HEAD in untouched files.
- `apr eval`'s hook points exist: `SetFitEvalArgs` at `setfit.rs:89`, `check_split_flags` at 254, `run_test` at 402 ending in `report(args.json, &row)` at 485, `evaluate_test` at 509; `force` on the `Eval` variant at `extended_commands.rs:197`.

**Concerns.**
- **MEDIUM. The pinned `apr` is never rebuilt with the setfit feature in this phase.** 10-06's verify built `cargo build --release -q --bin apr --features setfit` and checked `apr eval --help` lists the flag; 12-06 drops that step, and 12-10 only sources `scripts/apr_bin.sh`, which proves a binary is from HEAD but does not build it (`apr_bin.sh:75,317,437` talk about features only diagnostically). If the last `apr` build predates 12-06 or lacks `--features setfit`, the s8 rehearsal fails on `--dump-probabilities` or `apr setfit`. Put the build back here or at the top of 12-10.
- **LOW. `--validation-fraction` default stays 0.1**, so the carve on 41 conspiratorial train rows gives 5 validation rows; the plan's pool arithmetic (36) matches `ceil_share` (`:245-247`).

**Suggestions.**
- Re-add 10-06's final verify step (build the pinned binary with `--features setfit`, assert `eval --help` lists the option) to Task 2.

**Risk: LOW-MEDIUM.** The code change is well specified; the binary-build gap is a sequencing hole that 12-10 would hit.

---

## 12-07

**Summary.** Fixes the two numbers the gate run needs (epochs by a rule written before the probe; slice fraction by the folded spike on a train-carved holdout) and stops for the owner on a binding clamp. `probe_epoch.py` is as described: `GUARD_DEFAULT` at 61, `EPOCHS_RULE` at 64, hours keyed `"4"…"12"` at 291. Spike 027's harness exists at the cited skill path. Holdout and slice counts in the verify (75 = 17/11/47; spike-train 49/30/141; slice [13, 8, 36]; fit 163) all recompute correctly.

**Strengths.**
- Committing the README with both rules before the first cell, then asserting `started_at > committed_at` using the oldest README commit, is a clean, re-derivable declare-before-read proof.
- The `sys.addaudithook` guard on the gate's `eval.jsonl` path, with a selftest that must see the refusal, turns "the spike never read eval" from a promise into a measured fact.
- The slice and clamp rules are quantitative and the verify re-applies them to the recorded runs, so a mis-stated selection cannot pass.
- The checkpoint is conditional with a recorded skip, following 10-03.

**Concerns.**
- **MEDIUM. Compute spend vs CLAUDE.md check-in rule.** The grid is about 1.5 h of MPS (research A1) and runs in Task 2 with no owner gate; CLAUDE.md says compute over 1 h on a non-lambda-vector host is a check-in. D-17 folds the spike into the phase, which arguably pre-authorises it, but the plan should say so in one sentence rather than leave the executor to decide.
- **LOW. The clamp rule's second trigger** ("holdout ECE at T=5 > 0.10 while oracle ECE ≤ 0.10") compares ECEs on about 11 conspiratorial rows; spike 027 saw ECE move by 0.048 between MPS replicates. The rule is declared, so it is honest; expect it to be noisy and say so in the README.
- **LOW. The holdout is carved by re-applying the eval rule to train**, so the spike's spike-train (220) differs from the gate's fit set (220 after the slice). That is fine for a measurement, but the README should state it is not the gate's fit/slice partition.

**Suggestions.**
- Add "the folded spike's about 1.5 h is pre-authorised by D-17 (owner, 2026-10-26)" to the plan context, or make the grid start a lightweight owner ack.

**Risk: LOW-MEDIUM.** Mechanism is sound; the risk is schedule and noise, both disclosed.

---

## 12-08

**Summary.** A branch plan: record a no-op at 0.25, or implement a per-run slice override keyed by the train sha. The hazard it avoids is real: `check_slice_classes` (`verify.rs:1717-1732`) enforces `calibration_slice_fraction` as a minimum for every run, and `laya-deploy`/`laya-upload` re-verify before every AWS action (`justfile:1987,2602`), so a global edit would strand the live antisemitism artifact.

**Strengths.**
- The no-op branch is a recorded, hash-bound decision, not a skipped step (the 10-11 pattern).
- Refusing fractions outside (0.25, 0.5] at policy load and binding the entry to the train sha prevents reuse on another dataset.

**Concerns.**
- **LOW. Dependency on 12-09 is editorial.** `depends_on: ["12-03", "12-07", "12-09"]` serializes contract derived-file regeneration (census, `.nt`, `.ttl`), which is sensible, but the plan should say that is the reason; otherwise a reader looks for a door-to-slice coupling that does not exist.
- **LOW. The no-op verify's `git diff HEAD --name-only` must be empty across three directories.** Any stray uncommitted file from a concurrent wave in `scripts/laya_train/` would fail it for the wrong reason. Acceptable at wave 4 when prior waves are committed.

**Suggestions.**
- One sentence in the objective: "12-09 precedes this plan only so the two contract edits do not race on `contracts/census.json`."

**Risk: LOW.**

---

## 12-09

**Summary.** Keeps `decide-tool-boundary-v1` true for two served tasks (additive `served_tasks` record, both branches) and, under variant (b), derives the door per artifact. The premise is verified: `build_server` serves only `ClassifyLimits::CONTRACTED` (`lib.rs:779-781`), `build_server_with_limits` is private (`:786`), and `check_served_task_fits` refuses at build when `max_texts × min_row > 800` (`:751-766`). Contract is 10.0.0 with constants at 302/311/326 and highest id FALSIFY-DECIDE-TOOL-009.

**Strengths.**
- Keeping `served_task_min_row_tokens: 86` and adding `served_tasks` is the honest way to describe two live tasks without re-deciding the antisemitism door.
- `every_served_task_fits_its_door` makes the record a mirror test, not prose.
- Branch (b) routes the probe's replay range, over-limit count and distributed shape through the same `served_for`, keeping OPS-03.

**Concerns.**
- **MEDIUM. Branch (b) misses `parse_args`.** `parse_args` checks the array length against `ClassifyLimits::CONTRACTED` before deserializing (`lib.rs:727-730`), independently of the service's limits. Under a derived door of 4, a 6-text call passes `parse_args` (6 ≤ 8) and is refused by `precheck` with the service's limits, while a 9-text call is refused by `parse_args` naming 8. Two bounds, two messages. `served_for` must also feed `parse_args` (make it take the limits), and `one_over_max_texts_refused_naming_max_texts` (`tests.rs:251`) needs a derived-door case.
- **MEDIUM. Branch (b) scope.** A MAJOR contract bump plus `tool_description_for`, probe, README, claims rows, and `max_texts_fit_the_budget_at_the_shortest_row` (`tests.rs:189`), which currently assumes one task. Phase 10 priced the same change as a multi-day item (`10-LEARNINGS.md:35`). The plan's estimate (60k tokens, 2 tasks) is for branch (a).
- **LOW. `every_served_task_fits_its_door` on branch (a)** asserts `8 × P ≤ 800` for the motivation entry, which 12-01 already guarantees (P ≤ 100). Fine.

**Suggestions.**
- Add `parse_args(args, limits)` to the (b) action list and a test that a count of `door + 1` is refused naming the derived door.
- Split the estimate by branch, or make branch (b) its own plan inserted at the D-09 checkpoint.

**Risk: LOW for (a), MEDIUM-HIGH for (b).**

---

## 12-10

**Summary.** Adapts the motivation split, rehearses at s8 under a declared 32 GiB memory rule, then runs the declared s32 cell with the probability dump, or records a blocked arm. The recipe facts hold: `laya-setfit-cell data out config sel` at `justfile:1351` with `--shots 64` hardcoded at `:1370`, the `selection-s64-seed13` default at `:1363`, `minilm_dir` at `:26`, and the OK line at `:1388`. `ALLOWED_SHOTS [8, 16, 32, 64]` at `select.rs:59`; pools after the carve (36/59/169) make s32 the largest legal cell.

**Strengths.**
- The memory rule is written into the evidence skeleton before the rehearsal, and the verify recomputes the decision from the logged `maximum resident set size`.
- The same-pass control (accuracy from the dump bit-equal to `apr eval`'s value, predicted equals argmax) is the right proof that the dump is not a second classify path.
- The D-25 deviation ("subsampled train, 96 of 295") is a mandatory evidence string.

**Concerns.**
- **MEDIUM. `apr` build with `--features setfit` is assumed, not performed** (see 12-06). The recipe sources `apr_bin.sh`, which refuses a stale binary but does not build one.
- **LOW. `/usr/bin/time -l` wraps `just`, not `apr eval`.** Phase 10 timed `apr eval` directly (`10-05-SUMMARY.md:106`). macOS `time -l` on a parent reports the children's rusage as the parent waits for them, so the grandchild's peak usually propagates, but the plan should include a one-line control (a known allocation) or time the eval step directly, since the 32 GiB rule hangs on this number.
- **LOW. The rehearsal consumes a test-split read** on the s8 artifact's lock; nothing in `run_test` consumes the lock (10-06 context), so the s32 cell's own read is unaffected.

**Suggestions.**
- Add `CARGO_INCREMENTAL=0 cargo build --release --bin apr --features setfit` as step 0 of Task 1, then `. scripts/apr_bin.sh`.
- Record `max_rss_bytes` from a direct `/usr/bin/time -l "$APR" eval …` of the rehearsal's test split as the number the rule reads.

**Risk: LOW-MEDIUM.**

---

## 12-11

**Summary.** The one declared gate run: declaration committed first (waiver with four hashes and the recipe id, a `motivation_v1` run block), then exactly one `laya-train`, then pack, one verify, inspect, plan-only door check, with a halt checkpoint for every non-eligible outcome. It is a careful adaptation of 10-04, whose record (`10-GATE-RUN-EVIDENCE.json`) shows the shape works.

**Strengths.**
- `recipe_id_for` with `shots_per_class = 188` matches how `train.py` derives it (`max(class_counts)`, the same call `probe_epoch.py:162` makes), so the declared id can equal the written one.
- The verify ties `declaration_commit.committed_at < started_at`, `RECIPE WRITTEN` count 1, three equal `DEVICE` lines, rc per outcome, and `waived` consistent with `gate.pass`; every assertion is re-derived from logs and files.
- Replacing `waiver_real_contract_declares_nothing_yet` in the same commit as the declaration is correctly anticipated.
- `plan_only` checks avoid the `tokens_planned == door × P` trap (the carried lesson).

**Concerns.**
- **LOW. Exit code on a waived run.** `train.py` exits 3 on GATE FAIL (`:930`), and 12-03 keeps that; `just laya-pack` exits 3 on `GateFailed` but will accept a waived run through `verify_loaded`. The verify's `want_rc["gate_fail_waived"] = 3` is right; the plan should state plainly that rc 3 followed by a successful pack is the expected shape, so an executor does not stop at the "GATE FAIL" line.
- **LOW. Zero-shot margin under variant (b).** The research's Q1 pitfall (a fuller instruction raises zero-shot macro-F1 and can shrink the margin) is a real D-20 risk; the plan correctly measures it only after the run, but the owner checkpoint in 12-01 is where it should be stated, and it is.

**Suggestions.**
- Add to Task 2: "rc 3 with `PILOT WAIVER motivation-v1-pilot-ece` in the log is the gate_fail_waived path; proceed to pack."

**Risk: MEDIUM** (inherent: one declared MPS run on 41 conspiratorial train rows), well contained.

---

## 12-12

**Summary.** The live deploy under D-23: read-only antisemite baseline, config, offline selftest for the id, upload; owner login; deploy with the 404-retried health, identity probe, negative control, readback, and the antisemite server shown unchanged. Recipes exist with the cited signatures; `aws configure get region --profile ze-kasher-dev` returns `us-east-1`; the credentials file exists; `models/decide/swap-backup` exists.

**Strengths.**
- Upload/deploy split around the token gate follows the Phase 10 lesson (`10-LEARNINGS.md:153-157`).
- The negative control (antisemitism artifact refused by the new endpoint) and the seven-key before/after comparison of the antisemite function reuse the 10-12 shape exactly.
- Account ids and bucket names are scrubbed and regex-checked.

**Concerns.**
- **MEDIUM. The executor may be refused.** In Phase 10 "the executor's launch of the same command was refused by the Claude Code permission classifier ([Production Deploy])" for both upload and deploy, and the owner ran them (`10-RENAME-DEPLOY-EVIDENCE.json:174,592`; `10-12-SUMMARY.md:128`). Plan 12-12 has no `run_by: owner` branch in `upload` or `live_attempts`, and its Task 3 verify assumes the recipe log at `/tmp/p12-12-t3-deploy.log` exists. Add an owner-run branch (command text, the owner's reply, the executor's read-only confirmation of the S3 object and function) as 10-12's record did.
- **LOW. The working-tree justfile is what runs.** `laya-deploy-config` in the tree carries the owner's uncommitted `region="us-east-1"` parameter (`justfile:1701`). Calling with five arguments uses its default, so behaviour is unchanged, but the evidence should record that the recipe executed from a tree containing that diff (D-23 forbids depending on it, not running beside it).
- **LOW. `lockf -k -t 300` for the deploy** can fail with exit 75 while a 21-minute verify holds the lock elsewhere; the plan handles this as "repeat the token gate", which is correct.

**Suggestions.**
- Mirror 10-12's `live_attempts[].run_by` and `upload.run_by` fields, and make Task 1's and Task 3's verifies accept an owner-run attempt whose effects the executor confirms read-only.

**Risk: MEDIUM.** Outward-facing and credentialed; the plan's controls are good, the classifier fallback is missing.

---

## 12-13

**Summary.** Cold/warm timings via `laya-deploy-verify` (exists at `justfile:2787` with `samples="2"`), normalized with derived `proven_cold`, then the acceptance replay with the partner's batch of 4 and the count refusal at door + 1. `REPLAY_TEXTS` is eight texts with cycling for larger n (`probe.rs:693-725`), so the over-limit call at door + 1 is always constructible.

**Strengths.**
- `proven_cold` derived from the CloudWatch line and probe id, never copied, repeats 10-12's normalization.
- The step-down rule (only on `classify_max_total_tokens`) and the 4-text call directly test experiment-coach's documented behaviour (commit 52f31d7).
- A failed verify leaves the function serving and routes to the owner; the verify refuses to run teardown.

**Concerns.**
- **LOW. `requester_tests` cardinality.** The verify requires exactly seven keys; the plan's list is seven. Any later addition needs both edited.
- **LOW. Slack comparison.** "Beside Phase 10's 4.5 s" is reported, not gated; with P up to 196 the per-call compute is still token-bounded (800), so the slack should be similar. Fine.

**Suggestions.**
- Record the `graviton` field per cold sample in `live{}` as 10-12 did; the generation lottery (graviton2 vs 3) explained a 5.7 s spread there.

**Risk: LOW.**

---

## 12-14

**Summary.** Brings the Kev-4B arm to a decision: disk measured around the authorised `cargo clean`, fp32 merge and f32 GGUF, spike-033 driver built against the in-place branch, torch parity on public rows, guide prompt without §4.5, rows encoded from the shared split, one timed row, projections, owner checkpoint. Host facts: worktree on `spike/016-upstream-sync` at `32103ba8…` with only `Cargo.lock` modified; `target/` is 19.0 GiB; free disk is **43.3 GiB today** (research measured 25); kev vendor with `.venv`, llama.cpp converter, kev-4b adapter and Qwen3.5-4B-Base cache all present; `data/tweet-eval-stance/test.jsonl` and `015/tools/tasks.py` exist for the parity fixture; spike 020's `Cargo.toml` path-depends on the worktree (`:11-12`).

**Strengths.**
- Parity against the torch fp32 oracle on public rows before any motivation row is the right gate; a failure is `blocked (parity)`, never worked around.
- Leakage counts (eval posts in guide text; fit rows in eval) are asserted to 0, and rows carry ids only.
- The budget checkpoint sits before any multi-hour compute, with projections from a measured warm row.

**Concerns.**
- **MEDIUM. Host memory for the fp32 merge.** `export_and_oracle.py:27-33` holds the merged fp32 model and a second fp32 `AutoModelForCausalLM` simultaneously; at 4B that is two 16 GB copies plus PEFT overhead on a 48 GB host. The plan records an OOM as `blocked (memory)`, which is honest, but a cheap change avoids it: write `m.lm.state_dict()` directly as the HF safetensors shards (keys prefixed `model.`) beside the base's config and tokenizer, instead of instantiating a second model.
- **MEDIUM. Rust peak RSS at 4B f32.** 0.8B f32 (3.0 GB file) peaked at 7.2 GB RSS (017 RUN-DTYPES); a 16.8 GB file scaled the same way approaches 40 GB, and a 7k-token row's activations add to it. The plan's `blocked (loader | runtime)` branches cover it, but the owner should see this projection at the checkpoint, not only the hours.
- **LOW. Prompt regime.** 6.8k-token branches are 6.6× Kev's 1,024-token training branch (`kev/model.py:13-15`); the plan records it. Any Kev number is a reference, not a product claim (D-06), so this is disclosure, not a defect.
- **LOW. Disk math improved.** With 43 GiB free plus 19 GiB from `cargo clean`, the 34 GB transient fits comfortably; the verify's `after > before` still holds.

**Suggestions.**
- Replace the second `from_pretrained` with a direct safetensors write of the merged backbone (plus copied config/tokenizer) to halve peak memory.
- Add `projected_peak_rss_bytes` (2.4× file size) to the checkpoint context.

**Risk: MEDIUM.** Never run at 4B in Rust; every failure mode has a recorded branch.

---

## 12-15

**Summary.** Scores the owner-chosen rows through the 4B prefill, fits bias and temperature on train rows only, writes eval probabilities in the shared schema, and cleans up its own intermediates. `fit_bias` (`015/tools/fewshot.py:52-57`) and `gate.fit_temperature` (`gate.py:65`) are as cited.

**Strengths.**
- Resumable dump keyed by `(split, row)` makes a crash mid-run recoverable without rescoring.
- `data.calibration_split(…, 0.25, 2, 13, 3)` on the scored train rows mirrors Laya's own calibration rule, so the Kev ECE is comparable in method.
- Deletions are restricted to `models/kev/` with sha256s recorded and the regeneration route named.

**Concerns.**
- **LOW. Subset option fit size.** 60 train rows → fit 45, slice 15 (5 per class); a temperature fitted on 5 conspiratorial rows is a weak number. It is a reference arm and the record says so.
- **LOW. `fit_bias` provenance.** Importing from the 015 tools under the laya_train env pulls `sklearn` via `fewshot.py`'s module imports; the plan's fallback (copy verbatim with a provenance comment) is the likelier path. Fine.

**Suggestions.**
- Store the zero-shot eval probabilities file's sha256 in the evidence as well, so bakeoff can optionally report a `kev_zero_shot` arm beside the fitted one (mirrors `laya_zero_shot`).

**Risk: LOW-MEDIUM** (bounded by the owner's budget).

---

## 12-16

**Summary.** Runs the bake-off once, writes the hand-off led by the false-assignment rate, and names the second served model in CLAUDE.md and the lambda README from records. The 10-HANDOFF shape is followed closely; the verify checks the key identities (sha, labels, server id, profile/region, draft designation, docx sha, UNMEASURED, pilot, softmax, π counts, GoldStandard caveat, Kev branch).

**Strengths.**
- Laya's arm must equal the gate numbers bit for bit (`float(...).hex()`), which is achievable because both derive from the same f32-widened probabilities with the same `math.fsum` metrics.
- Blocked arms propagate as `blocked` with their failure class, never as numbers; the verify refuses the reverse.
- CLAUDE.md edits are checked by `test_documented_paths_exist` (`readme_contract.rs:522`), and the claims ledger by `laya-claims-check` (`justfile:3695`).

**Concerns.**
- **LOW. CLAUDE.md paragraph length.** The decision-model row is already long (`CLAUDE.md:239-286`); a second paragraph with ten numbers risks the "derive, don't quote" rule the file opens with. Keep it to identity, gate verdict, and record names; leave rates to the hand-off.
- **LOW. Hand-off needles.** `"4.5"` and `"66"` are trivially satisfiable; the stronger checks (sha, docx sha, task sha, UNMEASURED) carry the weight. Fine.

**Suggestions.**
- Add the measured door and P to the CLAUDE.md sentence only if the owner chose (b); under (a) "at most 8 texts and 800 built tokens per call" already covers both servers.

**Risk: LOW.**

---

## Cross-plan assessment

**Dependency ordering.** The spine (01 → 07 → 08 → 11 → 12 → 13 → 14/15 → 16) is correct and every one-way step sits behind a blocking checkpoint. 12-03 and 12-07 run in the same wave on `train.py`; 12-03's AST check of the three training functions makes the coupling claim verifiable. 12-14's `cargo clean` is correctly placed after the deploy and acceptance work, and 12-16 budgets the rebuild. 12-08's dependency on 12-09 is editorial (contract derived files) and should be labelled as such.

**Scope.** Branch (b) of 12-09 is the one place scope can balloon (MAJOR contract bump, `parse_args`, probe, docs); it is correctly gated on the D-09 checkpoint. The Kev arm is bounded by two owner checkpoints and honest blocked branches.

**Security.** Credentials are never read beyond `expires_at`; every AWS write is through an existing recipe with offline selftests; account ids and bucket names are scrubbed and regex-checked; no post text leaves the machine (sentinel tests and 395-post scans in 12-01, 12-14, 12-16). The waiver path is fail-closed by construction (margin un-deserializable, four-hash binding, manifest rung-4 check).

**Performance.** Real-weights legs: 12-03 (21 min), 12-07 (about 1.5 h spike), 12-11 (train under 1 h, pack and verify minutes at 100 rows), 12-08 (21 min, only if implemented), 12-12 (upload verify, deploy verify), 12-13 (four cold starts). All serialized by the host lock. Kev is owner-budgeted.

**Does the set achieve the phase goal?** Yes: a pilot `motivation-v1` served as `aprender-mcp-decide-motivation` in the Phase 10 pattern, measured against SetFit and Kev on one declared split, with the pilot/gate status, prior correction and blind spots stated. The three findings above are fixable in the plan text before any execution.

**Overall risk: MEDIUM.** Driven by the 12-02 fixture defect (would surface as an unexpected RED), the 12-12 executor-refusal gap (would stall a human checkpoint), and the inherent variance of one MPS gate run on 41 conspiratorial train rows (disclosed and handled by D-19/D-20).

---

## Consensus Summary

Both reviewers say the plan set reaches the phase goal: the declare-before-read approach, the train-only fitting and the
fail-closed waiver hold up against the source. Neither found a decision that was missed. They disagree on overall risk (Codex HIGH on most
plans, Fable MEDIUM overall). The difference is mostly that Codex traced failure and recovery paths further, while Fable checked host
and history facts (Phase 10's executor refusal, disk, memory). The consensus below is ranked by what survived the orchestrator's
verification (next section), not by the reviewers' own severity labels.

### Agreed Strengths
- The waiver's refusal site, the binding to four hashes plus recipe_id, and the `WaivableClause` enum that has only an ECE variant (so D-20 is enforced when the contract loads).
- `skip_serializing_if` on `gate.waiver`, so the deployed antisemitism-v1 artifact stays byte-identical, with a real `laya-verify` as proof.
- The 12-07 spike carves its holdout from train only, and the audit hook refuses any read of eval.jsonl.
- Train-only bias and temperature fitting for Kev; SetFit's same-pass dump; Laya's numbers must reproduce the gate bit for bit.
- The 404-only `/health` retry sits at the single GET at justfile:2068.

### Agreed Concerns (raised by both and verified)
1. **12-02: `fullest_copy` with a waiver claim breaks `every_run_field_is_bound_or_report_only`** (verify/tests.rs:2474-2477; the control `.expect("the fullest copy verifies")` would hit `WaiverWithoutFailure`). Use a separate accepted waived copy (an ECE-only failure plus a matching declaration), take the union for the bindings-table test, and run the mutation sweep over both bases.
2. **12-03: the bindings and leaf sweeps over `packed()` alone don't cover `/gate/waiver`.** `manifest_bindings_table_matches_manifest_leaves` resolves `report:` pointers against `packed()`'s report (ladder.rs:1290-1324), and `every_manifest_leaf_is_bound` (:1232) mutates on `packed()` only. Resolve and mutate the waiver row on `packed_waived()`.
3. **12-09 branch (b): `parse_args` keeps the compiled count bound** (`check_count(&ClassifyLimits::CONTRACTED, …)` at lib.rs:727-730), and `served_min_row_tokens` only works through `check_served_task_fits(model, limits)` (probe.rs:1033). That function refuses a task with P > 100 at 8 texts, so the plan's `served_for(served_min_row_tokens(model))` cannot derive a door of 4–7. Separate measuring the minimum row from checking the door, and pass the derived limits through `parse_args`, `ClassifyService::served` and the probe.
4. **12-06 / 12-10: nothing builds a fresh `apr` with `--features setfit`.** `apr_bin.sh` checks a binary; it does not build one. Add the build plus an `eval --help` check of `--dump-probabilities` before 12-10's rehearsal.
5. **12-14: memory at 4B.** `export_and_oracle.py:27-33` keeps two fp32 4B models in memory at once. Write the merged state dict directly, and show the owner the projected Rust peak RSS at the budget checkpoint.

### Single-reviewer concerns that survived verification
- **12-05 / 12-16 (Codex): bootstrap containment.** Requiring `lo <= point <= hi` (12-05 behaviour line 101; 12-16 verify line 129) can reject a correct result, because resampled ECE is biased upward (Codex reproduced point 0.0 with interval [0.006, 0.098]). Check only that `0 <= lo <= hi <= 1` and that the interval is deterministic.
- **12-05 / 12-06 (Codex): SetFit rows lack row identity.** Matching on truth labels cannot detect two swapped rows that share a label, and that would corrupt the length bands. Add a row id or text sha256 to the dump (the adapter already builds row ids, laya_dir_to_canonical.rs:304) plus dataset provenance, and align on it.
- **12-07 (Codex): failure paths can't be executed.** Task 3's verify always opens `12-SPIKE-EVIDENCE.json`, which does not exist after a probe OOM. Aggregation requires all nine cells. The cache is reused because a file exists, with no identity check. Define probe-failed, grid-incomplete and completed states with nullable evidence.
- **12-11 + 12-12 (Codex): rerun handling.** Nothing enforces the two-attempt cap: after a failed `-run2`, rerun-once is still offered and targets the same directory. 12-12 hard-codes `models/decide/laya-motivation-v1` for upload and deploy, so a successful `-run2` cannot be deployed. Read the run dir and artifact path from 12-GATE-RUN-EVIDENCE.json, and keep attempt records immutable.
- **12-12 (Fable): the executor's deploy launch can be refused by the classifier.** In Phase 10 the owner ran both `laya-upload` and `laya-deploy` (10-12-SUMMARY.md:128). Add `run_by: owner` branches to `upload` and `live_attempts`, with the executor's read-only confirmation. Also recheck the token's `expires_at` right before each live call (Codex).
- **12-13 (Codex): failed probe samples hold null `probe_id` / `elapsed_ms`** (justfile:2899-2905 `s and s.get(...)`). Normalise them with explicit null fields, take timing summaries from measured values only, and require the 4-text call's checks to pass, not just a truthy line.
- **12-14 (Codex): a renamed package with a copied Cargo.lock plus `--offline --locked` fails to build** (the lock names `qwen35-batched-prefill`, spike 020 Cargo.lock:991). Regenerate the lock offline after the rename, then build `--locked`.
- **12-15 (Codex): resume skips rows by `(split,row)` only**, with no binding to the model, head, prompt or driver, and no rule for a partial last line. Bind resumed output to an input-hash header and drop a torn last line. `gate.fit_temperature` returns `(t_fit, t_applied, clamp_hit)` (gate.py:65,91), so unpack it. Copy `fit_bias` instead of importing `fewshot.py`, which runs its argparser at import.

### Divergent Views
- **12-04 retry bound:** Codex says the 10-minute bound leaves out curl time (worst case is about 29.75 min, from 40 × 30 s timeouts plus sleeps); Fable finds the bound reasonable. Both are right: record the full worst-case bound, or cap the total elapsed time.
- **12-07 compute:** Fable flags the spike's ~1.5 h of MPS against CLAUDE.md's >1 h check-in rule. D-17 (the owner folded the spike in) authorises it; say so in the plan.
- **Overall risk:** Codex HIGH, Fable MEDIUM. The verified defects are all in plan text and verify scripts, not in the design, so MEDIUM is the better reading once they are fixed.

## Orchestrator Verification of Reviewer Claims

| # | Claim | Verdict | Method / evidence |
|---|-------|---------|-------------------|
| 1 | 12-02 fullest_copy + claim breaks binding sweep | CONFIRMED | verify/tests.rs:2474-2477 `.expect("the fullest copy verifies")`; 12-02-PLAN.md:225-226 keeps the policy permissive |
| 2 | 12-03 bindings test resolves against unwaived report | CONFIRMED | ladder.rs:1290 `let b = packed();` and every `report:` pointer is resolved against it (:1307-1324) |
| 3 | 12-05/16 bootstrap must contain point | CONFIRMED (math) | 12-05-PLAN.md:101 and 12-16-PLAN.md:129 assert `lo <= point <= hi`; resampled ECE (absolute bin gaps) is biased upward, so the point can sit below lo |
| 4 | 12-07 T3 crashes on OOM path | CONFIRMED | the 12-07 T3 verify opens `sys.argv[2]` (SPIKE-EVIDENCE) unconditionally before the trigger is computed |
| 5 | 12-09 (b) derivation circular via served_min_row_tokens | CONFIRMED | probe.rs:1033-1038 needs `limits` and calls check_served_task_fits, which refuses at lib.rs:756-763; 12-09-PLAN.md:134 composes `served_for(served_min_row_tokens(model))` |
| 6 | 12-09 (b) parse_args keeps CONTRACTED | CONFIRMED | lib.rs:727-730 |
| 7 | 12-12 hard-coded run dir blocks -run2 deploy | CONFIRMED | 12-12-PLAN.md:106,116,184 use the fixed `models/decide/laya-motivation-v1`; 12-11-PLAN.md:282 names `-run2` |
| 8 | 12-12 executor deploy refused by classifier in Phase 10 | CONFIRMED | 10-12-SUMMARY.md:128; 12-12 has no `run_by` field |
| 9 | 12-14 --locked fails after rename | CONFIRMED | 12-14-PLAN.md:129-134 (renamed package, `--offline --locked`); spike 020 Cargo.lock:991 names the old package |
| 10 | 12-15 fit_temperature returns a tuple | CONFIRMED (MEDIUM) | gate.py:65-66 docstring and :91 `return float(t_fit), float(t_applied), clamp_hit`; 12-15-PLAN.md:139 `T = gate.fit_temperature(...)` |
| 11 | 12-15 resume unbound to inputs | CONFIRMED | 12-14-PLAN.md:133 skips by `row`+`split` only |
| 12 | 12-06/10 no fresh setfit apr build | CONFIRMED | 12-06 runs only `cargo test -p apr-cli --features setfit` (:169); 12-10 only sources apr_bin.sh |
| 13 | 12-11 rerun cap / -run2 dead-end | PLAUSIBLE, not executed | follows from 12-11-PLAN.md:281-306 (the option set doesn't depend on the attempt number) |
| 14 | 12-13 nullable sample fields | CONFIRMED (source) | justfile:2899-2905 writes `s and s.get(...)`, which yields None on a failed sample |
| 15 | 12-05/06 SetFit row identity | PLAUSIBLE | the dump spec has six keys and no row id; the adapter has row ids at laya_dir_to_canonical.rs:304 (not opened) |

Nothing to REJECT this round. All 15 HIGH/MEDIUM claims checked are either confirmed or consistent with the plan text.

## Plan-checker findings to fold into the same revision (gsd-plan-checker, iteration 1)

- **BLOCKER, 12-01 T3 (line 477) and 12-04 T2 (line 242):** the D-23 foreign-diff verify checks each line for membership in all of HEAD:justfile. The owner's added lines `        else` and `        fi` are already in HEAD (re-measured by the orchestrator), so the check fails even when the commit is correct. Compare the committed range diff `git diff <start>..HEAD -- justfile` instead: it must contain none of the foreign hunks' added lines, and `git diff HEAD -- justfile` must still equal the foreign patch.
- **WARNING, 12-12:** the working-tree justfile carries the owner's uncommitted `region` code and is the one that runs (Fable says the same). Either run the AWS recipes from `git show HEAD:justfile` with `--justfile <tmp> --working-directory .`, or record that the working-tree recipe runs and gives the same output at the default. Fix the plan's truth line 27 either way.
- **WARNING, 12-08 T2 / 12-11 T1:** add the fresh `"$PV" census … | cmp -s contracts/census.json` step used in 12-02, 12-03 and 12-09.
- **WARNING, 12-09 branch (b):** nothing updates the door sentence in crates/aprender-mcp-decide-lambda/README.md:141,144 ("8 texts"). Assign it to 12-09 T2 or 12-16 T3 with a grep check.
- **WARNING, 12-RESEARCH.md:739:** mark `## Open Questions` as `(RESOLVED)`, each item pointing to D-23, D-24, D-25, D-26, 12-08 and the 12-07 rule.
- **INFO, 12-14 T2:** D-11's rule to remove a quoted eval post from the prompt has no executable branch. Either add one or accept a stop as the D-11 route.
