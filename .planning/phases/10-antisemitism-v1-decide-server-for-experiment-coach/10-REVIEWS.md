---
phase: 10
reviewers: [codex]
reviewer_instances: [fable]
failed_lanes:
  gemini: "requested as --gemini; this GSD has no gemini lane (the gemini binary is the Antigravity CLI). Lane antigravity ran and failed: RESOURCE_EXHAUSTED (429) Individual quota reached, resets in about 66 hours. NOT reviewed by Gemini."
reviewed_at: 2026-10-21T22:30:00Z
plans_reviewed: [10-01-PLAN.md, 10-02-PLAN.md, 10-03-PLAN.md, 10-04-PLAN.md, 10-05-PLAN.md, 10-06-PLAN.md, 10-07-PLAN.md, 10-08-PLAN.md, 10-09-PLAN.md, 10-10-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review — Phase 10

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts

<!-- gsd:plan-revision-conflicts:end -->

**Run notes.** Review prompt included the owner's 2026-10-21 note that batch size is not critical (amendment to D-05 and D-15 in `10-CONTEXT.md`). The explicitly requested Gemini lane did not run (see `failed_lanes`); the non-Anthropic pairing requirement is met by Codex alone. Fable is a reviewer instance on the claude adapter (`## Claude Review (fable)`), per the global instruction, not a lane.

## Codex Review

Read-only review of all ten plans against the repository. No files were changed, and no builds, tests, network requests, or git mutations were run. Findings below distinguish source-confirmed defects from implementation risks.

## 10-01

**Summary:** The data preparation and approval sequence is well designed, but the checkpoint does not implement the owner’s amendment. Every available choice still preserves the eight-text door and rejects richer instructions above P = 100.

**Strengths**

- Reusing the existing normalization, overlap and calibration functions preserves the training boundary: `data.py` detects normalized overlap, rejects conflicting training groups and selects whole calibration groups. Evidence: `scripts/laya_train/data.py:222`, `:263`, `:283`.
- Approval before writing the production task is valuable because training captures the original task bytes and checks for subsequent data changes. Evidence: `scripts/laya_train/train.py:747`, `:897`, `:912`.
- The write-once behavior has a sound precedent: differing bytes and foreign files are refused. Evidence: `scripts/laya_train/prepare_stance.py:161`.

**Concerns**

- **HIGH — The amended owner choice is absent.** Task 2 offers p44/p60/p90 or an edit that is rejected above P = 100. It never offers lowering `classify_max_texts` to permit fuller instructions. Evidence: `10-01-PLAN.md:319`, `:333`, `:351`. The actual server constraint is `max_texts × measured_min_row <= max_total_tokens`, rather than an unconditional P ≤ 100. Evidence: `crates/aprender-mcp-decide/src/lib.rs:751`.
- **MEDIUM — Measurement and acceptance are conflated.** Task 1 refuses an over-budget candidate before the owner can compare it under a proposed smaller batch. Evidence: `10-01-PLAN.md:213`. Measurement should expose both feasibility results.
- **MEDIUM — The proposed cut detector can reject valid options.** Exactly 48 raw option tokens fit; truncation occurs when more than 48 tokens are supplied. The builder uses `.take(OPTION_MAX_TOKENS)`. Evidence: `crates/aprender-decide/src/laya/builder.rs:173`. “Reaches 48” is insufficient proof of a cut.
- **MEDIUM — The privacy verification is sampled.** Checking only the first 50 tweets cannot establish the stated prohibition over the entire dataset. Evidence: `10-01-PLAN.md:304`.

**Suggestions**

- Change Tasks 1–3, their acceptance criteria, must-haves and evidence schema to record the chosen served maximum B alongside P.
- Present two substantive checkpoint options: retain B = 8 with P ≤ 100, or reduce B, including B = 1, with a fuller candidate that survives the existing head and option limits.
- Report raw versus retained token counts, post truncation rates and both options’ feasibility. Describe accuracy benefits as hypotheses until measured.
- Add synthetic sentinel tests covering stdout and stderr on successful and malformed-input paths.

**Risk Assessment:** **HIGH.** The current checkpoint would prevent the owner from selecting the newly authorized trade-off.

## 10-02

**Summary:** Extending the existing probe is appropriate, but the replay interface excludes single-post calls and needs stronger negative tests.

**Strengths**

- Reusing `Rpc` preserves the existing HTTP request construction, bearer handling and timing measurement. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:184`.
- Folding checks into `ok()` follows the existing probe’s explicit verdict mechanism. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:107`.

**Concerns**

- **HIGH — `--replay 1` is forbidden.** The proposed range 2..=8 cannot validate the owner-authorized single-text deployment. Evidence: `10-02-PLAN.md:146`.
- **MEDIUM — The proposed type name is wrong.** The source defines `CallTiming`, whereas the plan specifies `CallRecord`. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:72`.
- **MEDIUM — Replay placement must handle an existing early return.** Without `--maximal`, the example returns immediately after the identity probe. Simply appending replay after that block would make it unreachable. Evidence: `crates/aprender-mcp-decide-lambda/examples/probe.rs:219`.
- **MEDIUM — Numerical and shape failures lack negative tests.** Reordered expected labels and a wrong sha do not prove rejection of missing results, missing probabilities, invalid sums or negative probabilities.
- **LOW — The payload helper is described too broadly.** It currently parses `content[0].text`; it does not implement the claimed `structuredContent` fallback. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:297`.

**Suggestions**

- Accept replay counts starting at one, with an explicit bound appropriate to the target deployment.
- Use `CallTiming`, move the early return deliberately, and test the CLI path.
- Add malformed-response tests for every verdict boolean, including probability range and finiteness.
- Distinguish transport failures, identity failures and replay defects; a live failure is not automatically a client-code defect.

**Risk Assessment:** **HIGH** if the served maximum becomes one; otherwise **MEDIUM**.

## 10-03

**Summary:** The probe correctly reuses the real training loop and reads back the device. Its failure evidence and scheduling estimates need clearer treatment.

**Strengths**

- `train_seed` performs actual optimizer updates, logs the device used and returns measured training time and completed steps. Evidence: `scripts/laya_train/train.py:313`, `:349`, `:391`.
- The planned calibration exclusion matches the trainer’s validation sequence. Evidence: `scripts/laya_train/train.py:751`.
- The statement that E = 4 principally provides best-epoch restoration is consistent with the restore mechanism. Evidence: `scripts/laya_train/train.py:369`, `:386`.

**Concerns**

- **MEDIUM — The OOM checkpoint cannot pass its verification.** Task 2 explicitly supports `probe-oom`, but its automated verifier unconditionally requires rc = 0 and completed-training fields. Evidence: `10-03-PLAN.md:261`, `:268`.
- **MEDIUM — The timing projection omits recurring monitor work.** The probe uses `fixed_epochs`; production runs score the calibration slice and fit temperature after each monitored epoch. Evidence: `scripts/laya_train/train.py:338`, `:369`. Multiplying fixed-epoch step time does not include that work.
- **MEDIUM — Capped row lengths do not reveal lost post content.** An assertion that maximum length is ≤ 512 is always compatible with substantial truncation. Evidence: `crates/aprender-decide/src/laya/builder.rs:216`.

**Suggestions**

- Define separate completed and refused/OOM evidence schemas and verification branches.
- Record raw post lengths, retained post lengths and truncation counts with the approved task.
- Add monitor overhead and repeated downstream verification costs to the schedule assumptions.
- Keep the probe tied to the approved task sha; remeasure if task bytes change.

**Risk Assessment:** **MEDIUM.** The measurement mechanism is credible, but failure handling and date projections are incomplete.

## 10-04

**Summary:** The declared-run discipline is strong. The packed-artifact prefix check is incorrect and will reject valid work after an expensive run.

**Strengths**

- The trainer records each seed before selecting the median-ECE seed and preserves that selection in the report. Evidence: `scripts/laya_train/train.py:829`, `:847`, `:869`.
- Approved task hashes are bound into the report, and data changes are checked before report creation. Evidence: `scripts/laya_train/train.py:897`, `:912`.
- Keeping pack and eligibility verification separate matches their actual entry points. Evidence: `crates/aprender-decide/examples/pack_laya.rs:140`, `:174`.

**Concerns**

- **HIGH — Distributed maximal planning does not measure empty rows.** The implementation fills each row’s allocated share with synthetic text, then distributes remaining slack. It normally approaches the total budget. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:637`. Therefore `tokens_planned == 8 × P` is false except incidental boundary cases. Evidence of the invalid assertion: `10-04-PLAN.md:244`.
- **HIGH — Failure verification contradicts supported outcomes.** If packing succeeds and verification subsequently fails, the artifact already exists; the non-pass branch nevertheless requires its absence. Evidence: `10-04-PLAN.md:208`, `:251`.
- **MEDIUM — Aborted runs need not have one remaining checkpoint.** Checkpoints are deleted only after all seeds have finished and the median is selected. An interruption during seed processing can legitimately leave several. Evidence: `scripts/laya_train/train.py:817`, `:851`.
- **MEDIUM — Verification repeats a full real-weight rescore.** The automated check runs `laya-verify` again after the action already performed it, adding another substantial scoring pass. Evidence: `10-04-PLAN.md:235`.

**Suggestions**

- Add an explicit empty-state measurement through `Model::prepare`, and assert `P_rust == P_python`.
- Keep maximal planning as a separate legal-request test.
- Parameterize any empty-batch total as B × P.
- Preserve and identify an existing but ineligible artifact; prohibit deployment rather than requiring deletion.
- Make checkpoint-count assertions outcome-specific and budget repeated verification explicitly.

**Risk Assessment:** **HIGH.** The current success verifier can fail after hours of otherwise correct training and packing.

## 10-05

**Summary:** The adapter approach is appropriate, but the prescribed training command cannot run: `--config` is mandatory.

**Strengths**

- The public dataset constructor and shared assembly point support the proposed adapter without hand-written attestation logic. Evidence: `crates/aprender-contrastive-data/src/prepared.rs:267`, `:287`.
- Canonical loading verifies attested bytes through the existing boundary. Evidence: `crates/apr-cli/src/commands/data_contrastive.rs:284`.
- Deriving validation from train preserves the requested eval split.

**Concerns**

- **HIGH — There is no implicit reference configuration.** `config: PathBuf` is a required CLI argument, and the training implementation parses it immediately. Evidence: `crates/apr-cli/src/setfit_commands.rs:28`; `crates/apr-cli/src/commands/setfit_train.rs:642`. The assertion that omitting `--config` selects defaults is incorrect.
- **HIGH — Broad refusal handling can hide this orchestration defect.** The plan accepts a nonzero dry run as a comparison outcome. Missing required arguments must not be reported as evidence that the SetFit lifecycle rejected GoldStandard.
- **MEDIUM — The refusal verifier requires evidence that may not exist.** It asserts successful same-eval-split equality before branching on whether adaptation failed. Evidence: `10-05-PLAN.md:323`.
- **MEDIUM — Document-order parsing needs an explicit implementation and test.** Ordinary JSON object iteration is not a safe label-order strategy here. The decide task parser uses an ordered criteria representation expressly for this purpose. Evidence: `crates/aprender-decide/src/task.rs:163`, `:172`.
- **MEDIUM — Dry-run coverage is overstated.** Calibration-regime and evidence-threshold checks occur in the lifecycle after the dry-run return. Evidence: `crates/apr-cli/src/commands/setfit_train.rs:656`, `:684`.

**Suggestions**

- Supply an explicit, validated e1b16 CPU configuration and record its hash.
- Treat usage, build and missing-file failures as implementation failures; reserve “comparison refused” for identified domain refusals.
- Preserve partial evidence on each failure branch without asserting unperformed checks.
- Test label order using `not_antisemitic` before `antisemitic`, whose lexical order differs.
- A refused comparison should leave SC-4 incomplete unless the owner accepts that outcome.

**Risk Assessment:** **HIGH.** As written, the plan can produce no trained comparison while treating the result as acceptable.

## 10-06

**Summary:** The probability-based comparison is useful, but the example cannot access its proposed reader, and the metric naming can silently turn accuracy into macro-F1.

**Strengths**

- The library exposes row probabilities, truth indices and labels through `RowPredictions`. Evidence: `crates/aprender-train/src/train/setfit/apr_evaluate.rs:305`.
- The test evaluator reads test rows through a grant and verifies artifact identity. Evidence: `crates/aprender-train/src/train/setfit/apr_evaluate.rs:508`.
- The house ECE implementation is available and explicitly documented. Evidence: `scripts/laya_train/metrics.py:79`.

**Concerns**

- **HIGH — The example cannot call the bounded-reader API.** `setfit_io` is private, and its reader is `pub(crate)`. Cargo examples are separate crates. Evidence: `crates/apr-cli/src/lib.rs:60`; `crates/apr-cli/src/setfit_io.rs:67`.
- **HIGH — The CLI reports accuracy.** `EVAL_METRIC` is fixed to `Accuracy`, and test evaluation computes correct/seen. Evidence: `crates/apr-cli/src/commands/eval/setfit.rs:184`, `:545`. Writing that value into `macro_f1_apr_eval` is incorrect.
- **HIGH — The implemented control can pass without comparing any metric.** It checks equality only when the CLI tag contains macro/F1. Since the actual tag denotes accuracy, that branch is skipped, yet the control reports success. Evidence: `10-06-PLAN.md:137`, `:144`.
- **MEDIUM — Reusing an unchanged lock is not intrinsically refused.** `mint_test_token` takes `&self` and verifies integrity and artifact identity; it does not track a persisted consumed state. Evidence: `crates/aprender-train/src/train/setfit/lock.rs:594`. “Second access” is not an adequate diagnosis for an arbitrary refusal.

**Suggestions**

- Prefer adding probability export to the existing evaluation operation, sharing the bounded reader and grant path.
- Otherwise explicitly plan the necessary public helper and its source-file changes.
- Compare dump accuracy against CLI accuracy, and compute macro-F1 independently with the house formula.
- Use accurate field names and fail on unexpected metric tags.
- Disclose that Laya’s shipped seed was selected using eval ECE, whereas SetFit uses one declared seed. Evidence: `scripts/laya_train/train.py:829`.

**Risk Assessment:** **HIGH.** Both compilation and the validity of the comparison are affected.

## 10-07

**Summary:** Offline readiness before the deployment decision is a sound sequence. The plan must inherit the amended batch decision and clarify authentication preparation.

**Strengths**

- Re-proving the resolver is necessary: deployment compares the proof’s version against the installed tool and refuses mismatches. Evidence: `justfile:1835`.
- The selftest genuinely distinguishes a successful positive leg from a skipped one and records AWS calls. Evidence: `justfile:2306`, `:2334`.
- The blocking replacement/auth decision is explicit before live work. Evidence: `10-07-PLAN.md:164`.

**Concerns**

- **HIGH — Readiness still assumes the unchanged eight-text door.** The runtime maximum is hard-coded in `ClassifyLimits::CONTRACTED`. A larger approved P cannot be made deployable by a later YAML-only edit. Evidence: `crates/aprender-mcp-decide/src/lib.rs:124`, `:751`.
- **MEDIUM — Auth-on requires the probe’s actual credential mechanism.** Recipes invoke the probe without `--bearer`; the probe reads `APRENDER_DECIDE_PROBE_TOKEN`. Evidence: `justfile:1948`; `crates/aprender-mcp-decide-lambda/examples/probe.rs:41`, `:116`.
- **LOW — “Only the owner decision remains” is too strong.** The positive DRY_RUN leg does not exercise remote authentication or cold behavior. Evidence: `justfile:1866`.

**Suggestions**

- Include B, P, effective instruction retention and the matching runtime/contract revision in readiness evidence.
- Complete the chosen boundary change before the new artifact’s offline deploy check.
- Make auth-on readiness include secure token acquisition and the exact environment variable required by embedded probes.
- Preserve the existing answered-owner checkpoint before upload and replacement.

**Risk Assessment:** **MEDIUM**, rising to **HIGH** if the new task needs P > 100 and the boundary remains unchanged.

## 10-08

**Summary:** Artifact identity checks are strong, but the cold-verification recipe does not provide the containment behavior or evidence schema the plan assumes.

**Strengths**

- Deployment verifies eligibility before proceeding to live deployment. Evidence: `justfile:1843`.
- The deploy recipe checks edge routing and the live artifact identity and contains identity failures. Evidence: `justfile:1892`, `:1930`, `:1943`.
- Cold verification correlates a sample’s probe ID with a CloudWatch load line. Evidence: `justfile:2670`.

**Concerns**

- **HIGH — Timeout containment is not implemented by `laya-deploy-verify`.** A slow sample increments failures; the recipe ultimately prints `DEPLOY VERIFY FAILED` and exits. It does not call teardown. Evidence: `justfile:2686`, `:2731`. The plan’s `CONTAIN` alternative therefore does not match the actual failure path.
- **HIGH — Failure cannot legitimately produce `live.final: true`.** Task 2’s verification unconditionally demands final success and four valid samples even when the action says containment should halt. Evidence: `10-08-PLAN.md:177`, `:185`.
- **HIGH — The emitted evidence needs an explicit transformation.** Actual samples contain `cold_evidence`, `load_ms` and a log line; they lack several requested fields, including per-sample artifact sha and `proven_cold`. Evidence: `justfile:2715`. The plan cannot copy the object and claim those fields were recorded.
- **HIGH — The distributed `8 × P` assertion is incorrect.** Maximal planning fills the budget. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:637`.
- **MEDIUM — Auth-on wiring is incomplete.** The plan’s `DECIDE_BEARER` wrapper does not supply the environment variable used by probes inside the recipes. Evidence: `crates/aprender-mcp-decide-lambda/examples/probe.rs:116`.
- **LOW — Endpoint parsing reads the wrong TOML level.** The verifier reads a top-level `endpoint`; the recipe uses `deployment.endpoint`. Its fallback hides this discrepancy. Evidence: `justfile:2644`.

**Suggestions**

- Specify actual timeout behavior and explicitly implement any intended containment.
- Separate successful, failed and contained deployment evidence; only successful live verification enables hand-off.
- Map each normalized field to its original sample or log source.
- Correct maximal sizing, credential propagation and endpoint parsing before live execution.

**Risk Assessment:** **HIGH.** Failure handling currently promises behavior the source does not perform.

## 10-09

**Summary:** The hand-off covers the requested information, but replay sizes and client guidance remain incompatible with a one-text served maximum.

**Strengths**

- The planned identity chain aligns the local artifact, eligibility record, live deployment and partner pin.
- Reusing the existing RPC helper retains the documented request headers. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:184`.
- D-04’s pending shift probe and D-01’s deferred second model remain explicit.

**Concerns**

- **HIGH — Mandatory replay sizes cannot pass at B = 1.** The action requires sizes eight and two, while verification requires a successful n = 2 replay. Evidence: `10-09-PLAN.md:122`, `:141`. Count checking rejects these before classification. Evidence: `crates/aprender-mcp-decide/src/lib.rs:727`.
- **HIGH — The fallback handles token-budget refusal only.** Reducing the served maximum produces a count refusal, so trying 7..3 cannot recover.
- **MEDIUM — The batching formula is incomplete.** For equal-length texts it is `min(B, floor(800 / built_row_tokens))`; mixed-length batches require summing actual built-row costs. The server checks the sum, not the longest row alone. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:661`.
- **MEDIUM — Hand-off can precede the final contract/docs checks.** Its dependencies omit the final publication update. Evidence: `10-09-PLAN.md:6`.
- **MEDIUM — The partner-write assertion is not enforced.** Printing a note when the partner repository is dirty does not compare its before/after state. Evidence: `10-09-PLAN.md:135` onward.

**Suggestions**

- At B = 1, replay several distinct synthetic posts as separate one-text calls, and test B + 1 as an expected refusal.
- Record successful and expected-refusal cases separately.
- Generate hand-off limits from the deployed boundary, including B, row cap and truncation behavior.
- Release the hand-off only after all final contract/docs gates complete.
- Compare partner status against a baseline rather than requiring cleanliness or merely noting changes.

**Risk Assessment:** **HIGH** under the amended single-text option.

## 10-10

**Summary:** Updating claims with their anchors is good practice. However, this plan treats a potentially behavioral contract change as post-deployment documentation.

**Strengths**

- Contract and runtime equality is mechanically tested. Evidence: `crates/aprender-mcp-decide/src/tests.rs:145`.
- Moving ledger anchors with their source text avoids an immediate claims-check failure. The checker requires literal anchor presence. Evidence: `justfile:3497`.
- The separate README path check provides useful documentation verification. Evidence: `crates/aprender-core/tests/readme_contract.rs:542`.

**Concerns**

- **HIGH — A smaller served maximum must exist before deployment.** Changing YAML after the live deployment cannot change the already compiled `max_texts: 8`. Evidence: `crates/aprender-mcp-decide/src/lib.rs:124`. The present file list omits that runtime source.
- **HIGH — Contract-only changes fail the mirror test.** `bounds_match_contract` compares the YAML maximum directly with the Rust value. Evidence: `crates/aprender-mcp-decide/src/tests.rs:145`.
- **HIGH — The comments would record a false measurement.** Distributed maximal planning does not plan eight empty texts. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:637`; proposed comment: `10-10-PLAN.md:145`.
- **MEDIUM — Additional ledger rows must change at B = 1.** Rows 75 and 112 explicitly anchor the eight-text maximum and its published sentence. Updating only the named rows leaves drift. Evidence: `scripts/laya_claims.tsv:75`, `:112`.
- **MEDIUM — Evidence rows prove existence, not truth.** `#live.final` is traversed as a JSON key; the checker does not establish that it is true or that the anchored sha matches that deployment. Evidence: `justfile:3526`.

**Suggestions**

- Move behavioral boundary edits, runtime constants, affected tests and sizing support before artifact readiness/deployment.
- Keep measured live timing and retirement documentation after deployment.
- Replace false empty-row comments with a dedicated measurement command.
- Update every affected bounds anchor and add explicit sha/B/P equality checks beyond claims-ledger existence checks.
- Require final completion before publishing the hand-off.

**Risk Assessment:** **HIGH.** Current ordering cannot support the amended larger-guide option.

## Owner amendment: required changes and precise limits

**The two options are not presented fairly in 10-01.** Keeping P ≤ 100 is fully represented; lowering the served maximum is absent. The edit option explicitly rejects the latter’s principal benefit.

The revised checkpoint should compare:

| Option | Required implementation | Consequence |
|---|---|---|
| Retain B = 8 | Keep P ≤ 100 and verify instruction/option retention | Existing count policy remains; eight ordinary posts are still not guaranteed to fit |
| Reduce B, including B = 1 | Amend contract, Rust bounds, probes, tests and published limits before deployment | Allows fuller instructions within the existing builder’s effective head budget |

A smaller batch permits a larger prefix; it does not establish better accuracy. That remains a measured outcome.

The required plan changes are:

| Plan | Tasks and acceptance criteria requiring change |
|---|---|
| **10-01** | All three tasks: candidate measurements, checkpoint options, chosen B/P record, refusal rules and hard-coded `fits_8` assertions |
| **10-02** | Task 1 replay range and tests; preserve a way to exercise the current stance deployment separately |
| **10-03** | Record truncation/retention for the approved task and remeasure after any task change; training batch size is independent of served B |
| **10-04** | Task 2: replace maximal-as-prefix measurement; use explicit empty-row P and chosen B |
| **10-05 / 10-06** | Served B does not change their SetFit training/evaluation sizes; fix the separate command/API/metric defects identified above |
| **10-07** | Both tasks: readiness and owner deployment decision must identify the chosen boundary |
| **10-08** | Both tasks: deploy the matching binary/contract, use actual maximal sizing and correct failure evidence |
| **10-09** | Both tasks: successful replay sizes, expected count refusal, hand-off limit statements and batching formula |
| **10-10** | Split pre-deploy behavioral edits from post-deploy documentation; update all affected anchors and acceptance assertions |

### What happens around 192 and 512 tokens

P is the **built empty-state row length**, not the raw instruction length. `head_max_len = 192` budgets the head plus rendered options and their mask markers. Structural tokens make the maximum ordinary two-option empty-state row approximately **196 tokens** under the current builder. Evidence: `crates/aprender-decide/src/laya/builder.rs:181`, `:194`.

Consequently:

- Instructions can be cut **before their own length reaches 192**, because options consume the same budget.
- P slightly above 192 does not itself prove truncation; structural tokens explain the difference.
- Increasing the raw guide beyond the retained head budget silently discards its tail. The response’s `truncated` flag does **not** identify that instruction loss.
- Each rendered option independently retains only its first 48 tokens. Evidence: `crates/aprender-decide/src/laya/builder.rs:173`.
- When **P + post tokens > 512**, the post is right-truncated and `truncated` becomes true. Evidence: `crates/aprender-decide/src/laya/builder.rs:213`.
- Using the supplied maximum post length of 366 tokens, full retention requires P ≤ 146. At P = 196, only 316 post tokens remain.
- A built P above 512 is not achievable with the existing 192-token head budget. Enlarging that budget is separate builder/configuration work; extreme prefixes can also lose option markers and be refused. Evidence: `crates/aprender-decide/src/laya/mod.rs:329`.

Lowering B does not enlarge either the 192-token head budget or the 512-token row.

### Downstream correctness when B = 1

The downstream plans **do not remain correct unchanged**:

- **10-04:** `8 × P` must be removed. A dedicated empty-row check yields P; maximal planning yields a different measurement.
- **10-08 / 10-10:** Contract and Rust bounds must change before deployment. With one text and a 512-token row cap, a maximal request is at most **512 built tokens**, despite the retained 800-token total budget. Both maximal shapes collapse to a single-row case under the current implementation. Evidence: `crates/aprender-mcp-decide-lambda/src/probe.rs:619`, `:637`.
- **10-09:** Successful calls must contain one text. Multiple successful one-text replays plus an expected two-text refusal replace mandatory successful two/eight-text batches.

## Overall assessment

**Overall risk: HIGH — revise before execution.**

The plans cover the intended lifecycle and contain the necessary owner decision before replacing the stance deployment. D-01, D-03, D-04, D-06 and the deployment authorization sequence are substantially represented. However, amended D-05/D-15 are not implemented, SC-3/SC-5 have incorrect verification assumptions, and SC-4 can be undermined by command failure or mislabeled metrics.

The highest-priority corrections are:

1. Present and implement the smaller-batch option before training/deployment readiness.
2. Separate empty-prefix measurement from maximal-request sizing.
3. Supply SetFit’s required configuration and fix the probability-export API boundary.
4. Compare accuracy with accuracy and compute macro-F1 independently.
5. Correct deployment failure/containment handling and single-text acceptance replays.
6. Put final hand-off after all contract, documentation and verification work.

---

## Claude Review (fable)

All verification done. Writing the review now.

# Plan Review: Phase 10 (antisemitism-v1 Decide Server)

Reviewed read-only against the repo on 2026-10-21. Every mechanism claim below was checked in the cited file. Nothing was written, built, trained or deployed.

**Headline.** The ten plans are unusually well grounded (recipes, exit codes, contract constants and the gates-selftest drift rules all check out), but three things must change before execution:

1. **The owner's 2026-10-21 note is not yet reflected.** Plan 10-01 can neither measure nor present a guide longer than the current door allows, so the checkpoint cannot offer the choice D-05's amendment requires.
2. **A cross-check that three plans gate on is arithmetically wrong.** `probe --plan-only --maximal distributed` fills each of the 8 texts up to its share of the 800-token budget; `tokens_planned` lands near 800, not at 8 x P. The equality asserted in 10-04 Task 2, 10-08 Task 2 and 10-10 Task 1 fails for every P below 100.
3. **No plan owns the door edit.** If the owner lowers `classify_max_texts`, a compiled constant, a contract, a claims ledger and the replay mode all change, and that must land before the deploy (wave 5), not in 10-10 (wave 6).

---

## Owner note of 2026-10-21: assessment against the plans

### (a) What must change

| Plan / task | What is wrong today | Required change |
|---|---|---|
| 10-01 Task 1, `--measure` step 5 | Refuses with `REFUSED prefix-budget` whenever 8 x P > 800, so no draft above P = 100 can be measured or shown. The gate row `prefix-budget` hard-codes that refusal as its must-fail case. | Make the door a parameter: report `max_texts_that_fit` and `texts_per_call` for 1..8 at the p50/p95 tweet; refuse only on a real cut (`prefix-cut`, which is correct regardless of door) or on P + 1 > 512. Keep a door-aware refusal behind a flag (e.g. `--max-texts 8`) so the gate row still has a must-fail case. |
| 10-01 Task 1, step 4 (drafts) | Three drafts capped at 44/60/90. | Add at least one draft near the head cap (about 165 instruction tokens, P near 196) so the owner sees the accuracy-side option measured, with its texts-per-call column reading 1..4. |
| 10-01 Task 2 (checkpoint) | Context text calls 100 "the hard ceiling: above it the server refuses to build and every call is a 503". That is true only while `classify_max_texts` stays 8. Options are p60/p44/p90/edit. | Present two doors side by side (see (b)). Add an option id for "full guide, door lowered to N texts" with its cost list. |
| 10-01 Task 3 verify | Asserts `e["measured"]["P"] <= e["budget_P_max"] <= 100` and `m["fits_8"] is True`. | Replace with `P <= budget_P_max`, `P * chosen_max_texts <= 800`, and record `chosen_max_texts` in 10-TASK-EVIDENCE.json. |
| 10-02 Task 1 | `--replay N` rejects N outside 2..=8. | Range must be 1..=`classify_max_texts` (read from the contract constant, not a literal). |
| 10-04 Task 2 action and verify | `plan_only.tokens_planned == 8 * P`. Wrong for every P (see cross-cutting finding 1). | Assert the builder did not refuse and `tokens_planned <= 800`; cross-check P through a Rust-side empty-row measurement instead (suggestion below). |
| 10-07 Task 1 | Nothing offline exercises `check_served_task_fits` (lib.rs:751-766 runs only inside `build_server`, i.e. on Lambda). The selftest builds the bootstrap but never a server. | Add `DRY_RUN=1 just laya-deploy-verify <apr>` (justfile:2616-2619 runs `--plan-only` for both shapes with no network); a P the door cannot admit refuses there, before AWS. |
| 10-08 Task 2 verify | `l["plan_only"]["distributed"] == 8 * P`. | Same fix as 10-04. |
| 10-09 Task 1 | Runs `--replay 8` and `--replay 2`; AC requires an n = 2 replay. | n = `classify_max_texts` and n = 1; the "texts per call = floor(800 / (P + t))" sentence becomes "1 per call" under the lowered door. |
| 10-10 Task 1 | Rewrites `served_task_min_row_tokens` comments as "8 x P = TP"; keeps `classify_max_texts: 8`. | Under the lowered door: `classify_max_texts`, its comment, FALSIFY-009's prediction text ("DISTRIBUTED (8 texts ...)"), claims rows 75 and 112 (`scripts/laya_claims.tsv`), the README "8 texts or fewer" line, and the CLAUDE.md phrase "at most 8 texts and 800 built tokens per call" that the plan says to keep verbatim. |
| New plan needed (wave 2 or 3) | Nobody edits `ClassifyLimits::CONTRACTED` (`crates/aprender-mcp-decide/src/lib.rs:124-131`), which is a compiled constant asserted equal to the contract by `tests::bounds_match_contract` (FALSIFY-007 test list, yaml:640). | A "door edit" plan: contract + constant + ledger rows + README/CLAUDE.md sentences + `pv diff` bump, landing before 10-07's selftest and 10-08's deploy. |

### (b) Are both options presented fairly at the 10-01 checkpoint?

No. The checkpoint is written for one door. Three concrete reasons:

- The measurement tool refuses the second option's drafts outright (`REFUSED prefix-budget`), so the owner would see no measured P, no texts-per-call column and no cut check for any guide above 100 tokens.
- The context frames 100 as a property of the server rather than of a contract constant, and never names what lowering `classify_max_texts` costs: a MAJOR `pv diff` bump of decide-tool-boundary-v1, the compiled constant, ledger rows 75 and 112, the served tool description (derived from the limits, so automatic), the replay mode's range, the partner's `classify_batch`, and the warm-throughput change (the only warm figures on record are single-text calls, justfile:2626, so eight-per-call warm cost was never measured either way).
- It omits the fact that matters most for the accuracy side: the head cap. With two short descriptions the builder keeps about 165 instruction tokens and cuts the rest silently (builder.rs:181-196), so "wider door" buys at most about 165 instruction tokens, not an unbounded guide. The owner should see that number next to the 100-token door figure.

### (c) What breaks or degrades as P grows

Verified against `crates/aprender-decide/src/laya/builder.rs`:

| P region | Mechanism | Effect | Flagged? |
|---|---|---|---|
| 8 x P > 800 (P > 100 at the current door) | `check_served_task_fits`, lib.rs:751-766, called from `build_server` (:792) | The Lambda cannot build its server; every call is a 503; `laya-deploy`'s identity probe fails and the recipe tears the function down. Pack and verify do not catch it. | Loud, but only live. Offline only `probe --plan-only --maximal distributed` refuses (`MaximalError::OverBudget`, src/probe.rs:512-517). |
| Instructions past the head budget (about 192 minus option tokens, so about 165-170 tokens) | `head_ids.into_iter().take(keep)`, builder.rs:196; `OPTION_MAX_TOKENS` 48 per description (:27) | The served and trained prefix is shorter than the approved text. The artifact still hashes the full task.json, so the record says one thing and the model saw another. | **Silent.** Only 10-01's `prefix-cut` refusal detects it (Python side; the Rust builder is parity-proven so the same cut applies). Keep that refusal under every door option. |
| Past the 512-token row | Prefix is capped at about 196 by the head cap, so the prefix itself can never exceed 512. What shrinks is the state room: `room = max_len - prefix - 1` (builder.rs:214-215) | At P = 196 a tweet has 315 tokens of room; p99 is 130, max 366, so roughly the top 0.1 % of tweets are right-truncated. Training rows truncate identically (train.py:320 uses the same builder). | Yes for the text: `truncated: true` per result (builder.rs:216-219). |
| Any larger P | `collate` pads to the longest row (train.py:216-228) | Per-step time and MPS memory rise with P; the 10-03 probe measures it, so no plan change is needed, only awareness that E and the date re-price. | n/a |

### (d) Do the downstream plans stay correct if the served maximum becomes one text per call?

No, in four places, plus one that is wrong already:

- **10-04 plan-only cross-check** is wrong for every P today (finding 1 below). With `max_texts` 1 the distributed shape is one text filled to the 512-token row, so `tokens_planned` is about 512, again not 1 x P.
- **10-08 Task 2** asserts the same equality, and `laya-deploy-verify`'s two shapes become identical one-text requests (harmless, but FALSIFY-009's prediction text and the `shapes == {concentrated, distributed}` reading should say so).
- **10-09** cannot run `--replay 2` (precheck refuses "2 texts exceeds classify_max_texts 1") and 10-02's flag rejects `--replay 1`. The acceptance replay as specified cannot execute.
- **10-10** keeps `classify_max_texts: 8`, the CLAUDE.md phrase "at most 8 texts" verbatim (ledger row 112 anchors it), and comment arithmetic "8 x P". All three would be false.
- **10-02** is unaffected live (it runs against the stance server before any change) but its range literal must not be 2..=8.

The part that does stay correct: the gate run, pack, verify, SetFit cell and the hand-off's identity fields never read the door.

---

## Cross-cutting findings

**1. HIGH: `tokens_planned == 8 x P` is not what the probe computes.** `build_maximal_request` for `Distributed` gives each of the `max_texts` texts `share = budget / count` tokens and fills it with synthetic units (src/probe.rs:637-659). The live record confirms it: "distributed: 8 texts, 800 built tokens (planned 800)" (08-LIVE-REDEPLOY-EVIDENCE.json, attempt 3 `verify.shapes`). The contract comment "plans 8 empty texts to 456 tokens" (yaml:302-305) describes an older builder and the research copied it. Plans 10-04, 10-08 and 10-10 inherit the error; their gates fail on correct work. Fix: assert `texts == 8 && tokens_planned <= 800` (proves the door admits the task) and get P from the Rust side by adding `"min_row_tokens"` to the plan-only JSON (one `model.prepare(&[String::new()])` call, the same measurement `check_served_task_fits` makes). 10-02 already touches `examples/probe.rs`, so this is a natural place.

**2. HIGH: the door decision has no owner plan and the wrong wave.** See (a). If the owner chooses the lowered door at 10-01 Task 2, nothing between wave 1 and the live deploy edits `ClassifyLimits::CONTRACTED`. 10-10 is after the deploy and only touches `served_task_min_row_tokens`.

**3. MEDIUM: `claims-check` anchors will move under either door.** Row 81 and 111 are handled in 10-10. Row 75 (`classify_max_texts: 8`) and row 112 ("at most 8 texts and 800 built tokens per call") are not mentioned anywhere; under the lowered door `just laya-claims-check` goes red at the contract commit.

**4. LOW: tool-output filtering is handled consistently.** Every plan routes `cargo test`/`cargo run`/`make` through `rtk proxy` and keeps `$?` out of pipes. `rtk` is present at `/opt/homebrew/bin/rtk`. Verified `cargo pmcp --version` prints `cargo-pmcp 0.28.1`, so 10-07's `awk '{print $2}'` comparison against the proof works (justfile:1838-1841 does the same).

**5. Security posture (verified).** Tweet text never enters git: prepare prints counts and hashes (prepare_stance.py precedent), evidence verifies assert the CSV's first 50 texts are absent, the CSV is read in place (partner path exists, 11,311 rows), replay texts are a committed synthetic constant, and the `#prolife` README example is replaced. Account ids and bearers are regex-asserted absent. The partner repo is never written. No gap found.

**6. Decision coverage.** D-01..D-18 each have an owning task; SC-1..SC-5 each have a plan. The D-05 and D-15 amendments are the only decisions not yet honoured.

---

## 10-01

**Summary.** The strongest plan of the set on data discipline: it reuses `data.normalize`, `group_train`, `refuse_overlap` and `calibration_split` (data.py:61-77, 263-307) instead of a second normalizer, declares the rule before the run, writes once, and controls the tokenizer measurement against the contract's 57. Its weakness is entirely the owner note: the measurement refuses the very option the owner asked to see.

**Strengths**
- The `--measure` control (`P == 57, markers == 3` on the stance task) is a real control, not a self-reference.
- `prefix-cut` refusal targets the one silent failure in the builder (builder.rs:196, :27).
- The gate row design follows the drift check exactly: a thin wrapper recipe with no verdict word, the refusal owned by the script (justfile:2866-2875 would otherwise flag it), and `LAYA_GATES_ONLY` ending in `LAYA GATES ROWS OK 1 of` (justfile:3401). `row_` functions are indented four spaces (justfile:3215), so `grep -c '^    row_prefix_budget()'` works.
- Task 3's scratch-then-production sequencing keeps the production dir unwritten until approval.

**Concerns**
- HIGH: `REFUSED prefix-budget` at 8 x P > 800 makes the wide-guide option unmeasurable (owner note (a), (b)).
- HIGH: Task 3 verify hard-codes `budget_P_max <= 100` and `fits_8 is True`.
- MEDIUM: the drafts stop at p90; the head cap (about 165 instruction tokens) is the real accuracy-side maximum and is not offered.
- LOW: step 4 says to fetch arXiv 2304.14599 at execution time. Record the fetched text's sha256 in the docstring; a later re-fetch can differ.
- LOW: the Task 1 verify runs the three `--measure --csv` passes plus a full prepare; each loads the tokenizer. Fine, just not fast.

**Suggestions**
- Parameterize the door in `--measure` (`--max-texts N`, default from the contract constant) and print `texts_per_call` for both the median and p95 tweet for N in 1..8.
- Add a `p165` draft and a fourth checkpoint option, with the cost list from (b).
- Record `chosen_max_texts` in 10-TASK-EVIDENCE.json so every downstream plan reads the door from one place.

**Risk: MEDIUM** (HIGH until the owner-note edits land; the rest is sound).

## 10-02

**Summary.** A clean addition to the one probe: `run_replay` reuses `Rpc`, `classify_params` and `classify_payload` (src/probe.rs:150-245, 297-336) and sends the partner's bare `tools/call` with the two headers the partner's client uses (`crates/core/src/classifier.rs:110-113` in the partner repo). The loopback tests follow `probe_fails_when_response_labels_differ` (src/probe.rs:799-825). The live smoke against the stance server is the right order: before 10-08 replaces it.

**Strengths**
- `ok()` ANDs all six checks, matching the `ProbeReport::ok` discipline and its `ok_requires_every_check` test pattern.
- Live smoke bounded at 8 retries, 20 s apart, mirroring `laya-deploy`.
- Verify uses `rtk proxy cargo test` so the per-test `... ok` lines exist to grep.

**Concerns**
- HIGH (owner note): the 2..=8 range must become 1..=`classify_max_texts`.
- LOW: the plan names the timing type `CallRecord`; the crate's type is `CallTiming` (src/probe.rs:73). The executor will notice, but the interface block is wrong.
- LOW: the identity branch of the example returns early when `shape.is_none()` (examples/probe.rs:219-225); the replay call must be inserted before that return or it never runs. Say so in the action.
- LOW: with the stance task (P = 57) four replay texts of 8-20 words cost about 4 x 80 tokens, fine; with the new task at P = 90 an 8-text replay exceeds 800. 10-09 handles this by stepping down, but 10-02's unit test should include a `TokenBudget` refusal case so the step-down path is proven.

**Suggestions**
- Read the replay ceiling from `ClassifyLimits::CONTRACTED.max_texts` rather than a literal.
- Add `min_row_tokens` to the `--plan-only` JSON here (cross-cutting finding 1).

**Risk: LOW** once the range is fixed.

## 10-03

**Summary.** The probe driver is correctly separated from `train.py`: `train_seed(13, base, requested, question, fit_rows, k, epochs=1, stopping="fixed_epochs")` matches the signature at train.py:303, `info` carries `device_used`, `steps`, `train_seconds` (train.py:391-395), and the device is read back from the `DEVICE ... used=` line, not the request. The not-a-gate row is required and the plan adds it: the wrapper's `exit 2` on a missing task.json trips the drift check's `GATE` regex (justfile:2867), exactly as `laya-train` does (row 32).

**Strengths**
- `--dry-run` reproduces `train.main` step 1 call for call, including `resolve_epochs("production", spc, 4)` as a proof that the production rule applies.
- The early-stopping honesty note (earliest stop = 1 + 3 = 4, so at E = 4 it only restores) is correct against `EarlyStopper` semantics as described at train.py:369-379.
- Stamp-file check that nothing appears under `models/decide`.

**Concerns**
- MEDIUM: `train.train_seed` under `fixed_epochs` with `calib_rows=None` skips the per-epoch monitor, so the measured s/step excludes the monitor cost the real run pays (`calibration_logits` over about 2,129 rows per epoch, train.py:370). The projected hours should add it or state the omission. The plan's `tail_budget_hours` covers scoring, not the per-epoch monitor.
- LOW: the OOM path writes `oom: true` into the evidence and exits 2, but the verify requires rc 0 and `PROBE OK`. Fine as a halt, but the acceptance criteria should say an OOM is a CHECKPOINT, not a failed task.
- LOW: `epochs_rule` "6 when s_per_step <= 1.0" is reasonable but unexplained in the record beyond a research citation; add the arithmetic (hours at 6 vs 4) to `epochs_reasoning`.

**Suggestions**
- Time one `calibration_logits` pass in the probe (it is one forward over the slice) and add `monitor_s_per_epoch` to the evidence.

**Risk: LOW.**

## 10-04

**Summary.** Faithful to the 08-16 discipline: one declared run, rc from a file, STOP RULE, pack/verify/inspect on a pass, owner checkpoint otherwise. The pre-flight refusal path (`REFUSED epochs: above 16 shots/class (`, contract.py:236) is a genuine end-to-end proof that the data loads under train.py's own validators before `out.mkdir`. The run-dir triple read back from the evidence makes the rerun-once path auditable.

**Strengths**
- Thresholds asserted unchanged; `inputs_sha256.task_json` tied to 10-TASK-EVIDENCE.
- `just laya-inspect` exists (justfile:1453) and prints labels and variant as the plan expects (pack_laya.rs:221-243).
- Pitfall 4 (argmax tie at pack) is read before packing and never retuned.

**Concerns**
- HIGH: the plan-only cross-check `tokens_planned == 8 x P` (cross-cutting finding 1). The verify fails on a correct artifact.
- MEDIUM: `rerun-once` after a GATE FAIL is a second draw of the same recipe after reading the result. CONTEXT D-09 sanctions one re-run, so it is the owner's call, but 08-16 allowed a re-run only after an abort that read no score. The hand-off and gate evidence should state "two declared runs, the second shipped" when it happens, so the claim scope stays honest.
- LOW: the Task 2 verify re-runs `just laya-verify` (about 18 min) on top of the action's verify and the two verifies inside upload and deploy. Reuse the recorded line plus `shasum` instead, or accept the cost knowingly.
- LOW: `df_free_gb >= 10` while the research notes the volume was at 98 % with 26 GiB free in 08-16; the three transient checkpoints, the artifact and release builds fit, but a tighter day would not.

**Suggestions**
- Replace the equality with "builder did not refuse, `texts == 8`, `tokens_planned <= 800`" plus the Rust `min_row_tokens` cross-check.

**Risk: MEDIUM** (HIGH until finding 1 is fixed).

## 10-05

**Summary.** The right doors are chosen: `PreparedDataset::<Canonical>::from_labeled_rows` (prepared.rs:267), `encode_jsonl`, `DatasetAttestation::from_prepared` (attestation.rs:107), and `read_attested_canonical` needs only a numeric `schema_version` plus `dataset_attestation` (data_tweeteval.rs:667-700, data_contrastive.rs:284-300). `apr data select` validates shots and seed against the CLI constants, not the manifest (data_contrastive.rs:506-507), so the adapted dir needs no `few_shot` section. `apr setfit train` without `--config` uses `reference_defaults(seed)` (setfit_train.rs:343) and has `--dry-run` (setfit_commands.rs:107). The eval flags `--split`, `--lock-out`, `--selection-lock` exist (extended_commands.rs:174-187). The same-eval-split proof by normalized hash sets is the right assertion.

**Strengths**
- Validation carve from train only, deterministic through `rng::derive_key`/`bounded` (rng.rs:86-163), with the declaration computed from the rows.
- Refusal-of-record branch recorded verbatim with step attribution and no later step run.
- Never writes a `setfit-benchmark-claims-v1` bench row.
- The `setfit-cell` gate row is correctly placed in the FULL tier (`full_only`, justfile:3014) because the must-pass trains for about 30 min.

**Concerns**
- MEDIUM: the `apr eval` metric is accuracy, fixed at `EVAL_METRIC = Accuracy` (eval/setfit.rs:184), not macro-F1. The plan's `test_metric` line and the evidence field names imply the headline; the SUMMARY should say "accuracy from apr eval; macro-F1 and ECE from the 10-06 dump". 10-06 already guards this ("if the tag names macro-F1"), but 10-05's readers will not know until then.
- MEDIUM: every row's `source_split` must equal the role (split.rs:260-268, Gate 2). The action says rows "become `LabeledExample { ..., source_split }`" without stating the value; make it explicit per split, or the ladder refuses with `SplitRoleMismatch`.
- LOW: the `must_pass cell` case trains into `$W/setfit-out` but writes the selection manifest into the real `data/decide/goldstandard-2024-setfit/selection-s64-seed13` ("only if absent"). Fine, but the gates-selftest's backup/restore list (justfile:2826-2836) does not cover it; a failed case can leave a half-written selection dir that the next run treats as present.
- LOW: the adapter's `--force` semantics copy `write_outputs` (data_tweeteval.rs:775-800), good; make sure the manifest also refuses without `--force` (the plan says "any of the four", correct).

**Suggestions**
- Write `selection-s64-seed13` through `atomic_write` semantics or into `$W` for the gate case.
- Name the EvalRow metric in the evidence as `metric_tag` and avoid the word "macro" near it.

**Risk: MEDIUM** (the regime-threshold refusal is a real possibility and is handled honestly).

## 10-06

**Summary.** The intent (probabilities through the one grant-gated door, `evaluate_rows_from_artifact(credential, dataset, EvaluatedSplit::Test(&grant))`, apr_evaluate.rs:508-563) is correct and the macro-F1 control to 1e-9 is a good tie to the CLI value. The obstacle is visibility: the doors the example needs are crate-private in apr-cli.

**Strengths**
- Dump schema of exactly six keys with no text or ids; verify rejects extras.
- `laya_beside` copied from the gate record and re-asserted equal; claim scope and D-07 readout note mandatory.
- A refused second test access is recorded, not worked around.

**Concerns**
- MEDIUM: `read_attested_canonical`, `read_selection_manifest` (data_contrastive.rs:284, 614) and `attestation_bytes_from_manifest` (data_tweeteval.rs:667) are `pub(crate)`. An example is a separate crate and cannot call them. The plan tells the executor to "rebuild the dataset the way that function does", which creates a second reader of `benchmark-manifest.json`, the exact drift the crate's own comments forbid (data_tweeteval.rs:703-711). Also unverified: whether `setfit_io::read_setfit_apr_file_bounded` and `Selection::replay` inputs are reachable from outside the crate.
- MEDIUM: the action never says the selection must be replayed before `reload_verified_run_from_apr` (eval/setfit.rs:229-242 does `read_selection_manifest` then `Selection::replay`). Without it the credential cannot be minted.
- LOW: a second test-split access on the same lock after 10-05's `apr eval --split test` is probably admitted (nothing in `run_test` is single-use, eval/setfit.rs:402-430), but the note at :477-483 calls the test number "reportable exactly once per committed selection". Two test reads of the same artifact under the same lock is a policy wrinkle worth one sentence in the evidence.

**Suggestions**
- Instead of an example, add a `--dump-probabilities FILE` option to `apr eval --split test` (one door, no visibility problem, the macro-F1 control becomes trivially the same run), or expose a `pub` function in apr-cli's lib that the example calls. The first is cleaner under OPS-03.

**Risk: MEDIUM** (likely to stall on visibility unless the route changes).

## 10-07

**Summary.** The stale resolver proof is real (`models/decide/resolver-proof.txt` says 0.24.3; `cargo pmcp --version` prints 0.28.1) and `laya-deploy` refuses on exactly that mismatch (justfile:1836-1841). The DRY_RUN selftest with `LAYA_ELIGIBLE_*` arms the positive leg on the new artifact (justfile:2290-2294) and the resolver-proof check runs inside that leg, so the order in Task 1 is right. The D-13 checkpoint is correctly one-way, with the sha in hand.

**Strengths**
- Zero-AWS proof (`AWS CALLS: 0`, justfile:2336) before the owner is asked.
- Decision block records choice, auth, date, `decided_by`, `access_confirmed`.
- The `second-server-name` option is honest about halting the phase.

**Concerns**
- MEDIUM: nothing in this plan exercises the served-task door offline. The selftest builds the bootstrap (justfile:2323-2327) but never a server; `check_served_task_fits` first runs on Lambda. Add `DRY_RUN=1 just laya-deploy-verify <apr>` (justfile:2616-2619 runs `--plan-only` for both shapes, no network) so a task the door cannot admit refuses here.
- LOW: the verify asserts `! grep -q 'SKIPPED'` over the whole selftest log; the recipe prints `SKIP positive dry run` on some paths and `FULL-tier ... SKIPPED` wording exists elsewhere in the justfile. Here only the final-line check matters; grepping the exact last line is enough.
- LOW: `served_task_min_row_tokens` (P) is recorded in the evidence but the contract still says 57 until 10-10; the checkpoint context should say so.

**Risk: LOW** after the plan-only addition.

## 10-08

**Summary.** The live slice follows 08-30 command for command (`laya-deploy-config`, `laya-upload`, `laya-deploy`, `laya-deploy-verify`), records attempts in the 08-30 schema, and adds a negative control (the stance artifact must now fail identity). The deploy is gated on `decision.choice` and `access_confirmed` from 10-07. Timings are copied, not judged, per D-16.

**Strengths**
- Negative control on the retired sha is the right proof that the replacement happened.
- `DEPLOY VERIFY OK|CONTAIN` covers both the verdict (justfile:2732) and containment (`CONTAINED:`, justfile:2755).
- Auth-on branch keeps the bearer in an env var and asserts no `Bearer ` or `--bearer` in the evidence.

**Concerns**
- HIGH: `l["plan_only"]["distributed"] == 8 * P` (cross-cutting finding 1).
- MEDIUM: Task 1 is `type="tracer"`; under the executor's tracer gate the `<verify>` is re-run after the task. Here the verify only re-probes (no second deploy), so it is safe, but say so explicitly: a reader could think the deploy re-runs.
- LOW: the probe already reads `APRENDER_DECIDE_PROBE_TOKEN` (examples/probe.rs:41, 116); `DECIDE_BEARER` plus a wrapper is a second convention.
- LOW: `deployment.toml`'s key name for the endpoint is assumed to be `endpoint`; the fallback URL covers it.

**Risk: MEDIUM** (HIGH until finding 1 is fixed; the one-way deploy itself is properly gated).

## 10-09

**Summary.** The hand-off list matches D-17 and adds the D-14/D-15/D-04/D-07/D-18/D-01 statements with a verify that checks each by substring. The D-13 amendment follows the precedent form the claims ledger already anchors (rows 113-117 of the 08 context), and the extractor (`decisions.cjs`, bullet-colon grammar at :117-135) tolerates a continuation line beginning with an em dash.

**Strengths**
- One sha asserted equal across the gate, live and replay records and quoted twice in the hand-off.
- The condensed task text is asserted verbatim against the file.
- The partner's reversed fixture order (classifier.rs:346 in the partner repo) is called out as harmless for a by-name parser.

**Concerns**
- HIGH (owner note): `--replay 8` / `--replay 2` and the "n 2" acceptance criterion cannot run under a one-text door; the `floor(800 / (P + t))` rule and `classify_batch` sentence change meaning.
- MEDIUM: the hand-off's D-15 statement should carry the amended wording ("at most the served `classify_max_texts`, which may be 1") in either case; the plan still writes "classify_batch at most 8".
- LOW: the verify's `"floor(800"` and `"classify_batch"` needles will need to follow the chosen door.
- LOW: the `git status --porcelain` of the partner repo prints a note rather than failing; fine, since the partner tree may legitimately be dirty, but say that this is why.

**Risk: MEDIUM.**

## 10-10

**Summary.** The right split from 10-08: contract, README, CLAUDE.md and ledger move after the deploy, each row with its anchor in the same commit, with `pv validate`/`pv diff` through the cargo-run form (`pv` is not on PATH; `scripts/pv_bin.sh` exists as an alternative). The anchors and indentation the verify greps were checked: `  served_task_min_row_tokens: 57` and `  version: 9.0.0` are two-space indented (yaml:306, :3), row 81's anchor matches (laya_claims.tsv:81), the README example at :104 carries `#prolife`, and CLAUDE.md:260 carries "The model is live on pmcp.run".

**Strengths**
- `max_texts_fit_the_budget_at_the_shortest_row` reads the constant from the contract (tests.rs:188-197), so the edit is tested without touching Rust.
- The 08-30 `implemented_by` text is kept under "SUPERSEDED ARTIFACT" so rows 79, 80 and 82 keep anchoring.
- `readme_contract` and `laya-claims-check` both run after the docs edit.

**Concerns**
- HIGH (owner note): under a lowered door this plan is incomplete (`classify_max_texts`, its comment, FALSIFY-009's prediction, rows 75 and 112, the README limits, the CLAUDE.md phrase kept verbatim) and too late (after the deploy).
- MEDIUM: "TP = live.plan_only.distributed (= 8 x P)" repeats finding 1; the comment text written into the contract would be numerically false. Write `tokens_planned` as what it is (the distributed request's total) and state P separately from `min_row_tokens`.
- LOW: under either door, `served_task_min_row_tokens` changing from 57 to P is a constant change; `pv diff` will likely say MAJOR. The plan accepts whatever `pv diff` suggests, which is correct.

**Risk: MEDIUM.**

---

## Overall

| Area | Verdict |
|---|---|
| Phase goal and SC-1..SC-5 | Reachable; every criterion has an owning task with a real gate. |
| D-01..D-18 | All covered; the D-05/D-15 amendments are not. |
| Live replacement of the stance demo | Properly gated: 10-07 Task 2 is a blocking-human decision with the sha, cost and containment stated; 10-08 reads `decision.choice` and `access_confirmed` before any AWS write; the negative control proves the replacement. |
| Verify gates | Mostly sound and hook-aware. Three gates assert an impossible equality (10-04, 10-08, 10-10); two hard-code the old door (10-01 Task 3, 10-09). |
| Long steps | Budgets are stated and background-run; the extra `laya-verify` in 10-04's verify is avoidable. |
| Security | No tweet text, credential or account id path found. |

**Overall risk: MEDIUM.** Two edits unblock most of it: parameterize the door in 10-01 (measure, drafts, checkpoint, evidence) and add a wave-2 "door edit" plan that 10-07 and 10-08 depend on when the owner lowers `classify_max_texts`; then replace the `8 x P` equality in 10-04, 10-08 and 10-10 with a builder-did-not-refuse check plus a Rust-side `min_row_tokens`.

---

## Orchestrator Verification of Reviewer Claims

Each HIGH/shared claim was checked against the repo before ranking (agreement between reviewers is not evidence). Two of Fable's claims were refuted.

| # | Claim | Raised by | Check (method) | Verdict |
|---|---|---|---|---|
| 1 | `probe --plan-only --maximal distributed` fills each text to its share of the 800-token budget, so `tokens_planned == 8 x P` (10-04 T2, 10-08 T2, 10-10 T1) is false | Codex, Fable | Read `crates/aprender-mcp-decide-lambda/src/probe.rs:619-665`; live record `08-LIVE-REDEPLOY-EVIDENCE.json` shows `distributed: 8 texts, 800 built tokens (planned 800)` | **CONFIRMED** |
| 2 | `apr setfit train` has no implicit config: `--config` is a required path (10-05 command cannot run) | Codex (HIGH) | `crates/apr-cli/src/setfit_commands.rs:28-37`: `config: PathBuf`, doc "no partial config and no per-knob default"; `setfit_train.rs:640` parses it unconditionally | **CONFIRMED. Fable's "without --config uses reference_defaults" is REFUTED** (that fallback is in `resolve_config(Option)`, not reachable from the Train subcommand) |
| 3 | `laya-deploy-verify` does not contain the function on a slow sample (10-08 plan relies on "the recipe's 30 s containment rule") | Codex (HIGH) | Read justfile `laya-deploy-verify` failure path (lines ~2676-2741): a slow sample increments FAILS and the recipe prints `DEPLOY VERIFY FAILED` and exits 1; containment (`CONTAINED:`) exists only in `laya-teardown` | **CONFIRMED. Fable's "DEPLOY VERIFY OK/CONTAIN covers containment" is REFUTED** (it conflated `laya-teardown`) |
| 4 | Lowering the served maximum needs a compiled-constant change: `ClassifyLimits::CONTRACTED.max_texts = 8`, mirrored by `bounds_match_contract`; a YAML-only edit in 10-10 (after deploy) cannot work | Codex, Fable | `crates/aprender-mcp-decide/src/lib.rs:124-131`; `src/tests.rs:145-160` | **CONFIRMED** |
| 5 | 10-02 rejects `--replay` outside 2..=8, so single-text replay is impossible | Codex, Fable | `10-02-PLAN.md` lines 21, 146, 163, 247 | **CONFIRMED** |
| 6 | 10-01 `--measure` refuses `8 x P > 800` and hard-codes `fits_8`/`budget_P_max <= 100`, so the wide-guide option cannot be measured or approved | Codex, Fable | `10-01-PLAN.md` lines 121, 213-215, 241, 263, 351, 389 | **CONFIRMED** |
| 7 | 10-06 example cannot call `setfit_io` / `read_attested_canonical` (private module, `pub(crate)`) | Codex (HIGH), Fable (MEDIUM) | `apr-cli/src/lib.rs:61` (`mod setfit_io;`), `setfit_io.rs:67`, `data_contrastive.rs:284,614` all non-public | **CONFIRMED** (compile failure, so HIGH is right) |
| 8 | `apr eval` reports accuracy (`EVAL_METRIC = Accuracy`), so the macro-F1 control can pass vacuously | Codex (HIGH), Fable (MEDIUM) | `crates/apr-cli/src/commands/eval/setfit.rs:184` | **CONFIRMED** |
| 9 | Pack-then-verify-fail leaves an artifact but 10-04's non-pass branch demands its absence; checkpoint-count assertions too strict | Codex | not checked | UNVERIFIED (replan must read `10-04-PLAN.md:208-251` and the run path) |
| 10 | 10-08 evidence lacks requested per-sample fields (`proven_cold`, per-sample sha); endpoint read at the wrong TOML level | Codex | not checked | UNVERIFIED (replan to check against `justfile` ~2715, ~2644) |
| 11 | Exactly-48 option tokens are valid; the `prefix-cut` detector "reaches 48" may reject them | Codex (MEDIUM) | not checked | UNVERIFIED |
| 12 | Probe timing excludes per-epoch monitor cost (calibration logits over ~2,129 rows) | Codex, Fable | Not measured; consistent with `train.py` `fixed_epochs` with `calib_rows=None` | PLAUSIBLE, low cost to add `monitor_s_per_epoch` |

**Tell the replan to REJECT:** (a) Fable's claim that 10-05 may omit `--config`; (b) Fable's statement that `laya-deploy-verify` implements containment. Both are source-refuted above.

## Consensus Summary

Two grounded reviewers (Codex, Fable); no Gemini review exists. Both cite `file:line` evidence; neither carries the without-repo-access or without-citations marker.

### Agreed Strengths
- Data discipline in 10-01: reuses the back office's normalizer, grouping and calibration split, declares the rule first, writes once, and controls the tokenizer measurement against the contract's 57.
- The training probe is correctly a separate driver over `train.train_seed`, with the device read back; the gate run, pack and verify follow the 08-16 discipline (rc from a file, stop rule, owner checkpoint on a non-pass).
- Replacement of the live stance demo is gated by an answered owner decision (10-07 Task 2) before any AWS write, with a negative control on the retired sha.
- No path found for tweet text, credentials or partner-repo writes; verify commands are hook-aware (`rtk proxy`), with no `$?` through a pipe.

### Agreed Concerns (ranked by what survived verification)
1. **HIGH: the owner's batch-size note is not implemented, and no plan owns the door edit.** 10-01 cannot measure, show or approve a guide above P = 100 and presents only the single-door choice; lowering `classify_max_texts` needs a compiled-constant, contract, test, claims-ledger (rows 75, 112, 81, 111), README and CLAUDE.md change that must land BEFORE the deploy, not in 10-10; 10-02 forbids `--replay 1`; 10-09 requires 8- and 2-text successful replays. At a one-text door the maximal request is at most the 512-token row. (Source-verified: items 4-6 above.) Note the real accuracy-side ceiling is the head cap, about 165 instruction tokens and P about 196, not "unbounded".
2. **HIGH: `tokens_planned == 8 x P` is wrong** in 10-04, 10-08 and 10-10 (and the contract comment text 10-10 would write). Replace with "builder did not refuse, texts == B, tokens_planned <= 800" plus a Rust-side empty-row measurement (`min_row_tokens`). (Item 1.)
3. **HIGH: 10-05's SetFit train command cannot run** without a validated `--config` (e1b16 CPU), and its tolerance of a nonzero dry run can record an orchestration error as a "comparison refused" result. (Item 2; Codex only, source-confirmed.)
4. **HIGH: 10-06 will not compile or will mislabel**: crate-private readers (item 7), accuracy reported as macro-F1 (item 8). Prefer adding `--dump-probabilities` to the existing `apr eval --split test` door (Fable) or an explicit public helper.
5. **HIGH: 10-08's failure branch promises containment the recipe does not perform** (item 3), its verify demands `live.final: true` even when the plan says halt, and its evidence mapping is unspecified (item 10, unverified).
6. **MEDIUM: 10-03/10-04 failure-path and cost accuracy**: the OOM branch cannot pass its own verify; monitor cost missing from the hours projection; 10-04 repeats an ~18 min `laya-verify`; rerun-once should be reported as two declared runs.
7. **MEDIUM: 10-07** has no offline check that the door admits the task (add `DRY_RUN=1 just laya-deploy-verify`); auth-on must use the probe's `APRENDER_DECIDE_PROBE_TOKEN`. **10-09** hand-off can precede the final contract/docs gates.

### Divergent Views
- **Overall risk:** Codex HIGH, Fable MEDIUM. After verification Codex's severity is the better fit for 10-05, 10-06 and 10-08 (compile and false-containment defects); Fable's "HIGH until two edits" framing is right for the door.
- **SetFit config:** Fable asserts defaults apply; Codex asserts `--config` is mandatory. Codex is correct.
- **Containment:** Fable treats `laya-deploy-verify` as containing; Codex says it does not. Codex is correct.
- **Door-edit ownership:** Codex wants the behavioural edits split ahead of deploy inside the existing plans; Fable proposes a new wave-2 "door edit" plan that 10-07 and 10-08 depend on. Either satisfies the requirement; a separate plan keeps 10-10 documentation-only.
