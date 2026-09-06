---
gsd_state_version: 1.0
milestone: v1.0
current_phase: 06
current_phase_name: Native Time-Series Forecasting Stack
status: executing
stopped_at: Completed 06-06-PLAN.md
last_updated: "2026-09-06T14:39:32.830Z"
last_activity: 2026-09-05
last_activity_desc: Phase 06 execution started
state_head: 6afdcdeba39c208aae214b8ba77cb9cd5270f402
progress:
  total_phases: 6
  completed_phases: 1
  total_plans: 73
  completed_plans: 67
milestone_name: milestone
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-08-07)

**Core value:** A small labeled dataset can produce an accurate, fast, reproducible classifier that trains and runs entirely through Aprender's native Rust and APR lifecycle.
**Current focus:** Phase 06 — Native Time-Series Forecasting Stack

## Current Position

Phase: 06 (Native Time-Series Forecasting Stack) — EXECUTING
Plan: 7 of 9
Status: Ready to execute
Phase 05 is PLANNED — 13 plans in 8 waves, verification passed, then REPLANNED 2026-08-17
against `05-REVIEWS.md` (codex + gemini). The replan is targeted, not from scratch: eight
consensus findings were incorporated and six of Gemini's were rejected with in-plan rationale
(its `f1_average_for_classes(&[1,2])` "off-by-one fix" would have INVERTED the phase's headline
metric — the ordered labels really are `["none","against","favor"]`). Structural changes worth
knowing before execution: (a) 05-06 gains a tracer task that must PROVE the LoRA
adapter-save → fresh-process-reload → ordered-probability-vector route before any 9B compute is
spent — the code confirms the gap (`ClassifyPipeline::from_apr` builds FRESH LoRA layers,
`forward_only_tokenized` returns `(loss, class)` not a probability vector); (b) 05-11 and 05-12
now depend on 05-10, so the 80 expensive cells cannot be generated before the gate that judges
them exists (waves 5→6, 6→7, 7→8); (c) cold latency and inference peak RSS move to a dedicated
fresh child process with a true kernel high-water mark on both platforms, and train peak becomes
a separate, separately-labelled field.
Last activity: 2026-09-05 — Phase 06 execution started

**Phase 04 UAT ran 2026-08-16 at `b3f816c25` (macOS/arm64): 12 tests, 12 passed, 0 issues —
see `04-UAT.md`.** Every gate was executed in-session, not read off a SUMMARY: codec 17,
reload 17, lifecycle 5, spawned CLI ladder 3+1, parity 20/20, artifact 86, eval 20, serve 11,
feature matrix (core 246 / train 311 / apr-cli delta 18→79 / serve 11) with RUN legs and
two-sided negatives, `pv validate` 0 errors 0 warnings, `contract-audit-phase4` 15/15 bound
with **zero BIND- lines**, `setfit-api-boundary` PASSED with an executed MUST-MATCH control,
`bashrs-lint-makefile` honest at bashrs rc=2 with 1 control-refuted false positive.

**`04-VERIFICATION.md` was stale and is now reconciled, not rewritten.** It was written at
`0fb47958f` and never re-run; the five gap-closure plans it triggered (04-18..04-22) landed
afterwards, and `roadmap.update-plan-progress` had already flipped Phase 4 to Complete on
summary-count parity alone — the SIXTH occurrence of that false-completion defect. Status is
now `gaps_acknowledged` with the original verdict and its measurements UNAMENDED, plus an
`## Acknowledged Gaps` section. Closed since the report: SC4+SC5's shared BIND-004 (04-19),
WR-08/WR-09 (04-18), WR-10 (04-20), both D-04-11-A production survivors (04-21), F-07 (04-22).

**Four items are OPEN by explicit human ruling, and Phase 4 does NOT close them:**

  1. **F-10** — `CALIBRATED_REGIMES` admits only the phase-3 MiniLM slice, whose 97-row
     vocabulary cannot compute `probe_unicode`, so no user-reachable path produces a
     `setfit-apr-v1`. SC1 and SC3 remain UNMET; SC2's policy has only run over a substituted
     encoder/head; SC4's parity fixture is synthetic. Deferred to **Phase 5**, which must
     calibrate on production `all-MiniLM-L6-v2` and add its fingerprint to
     `contracts/setfit-train-lifecycle-v1.yaml` by deliberate `pv diff`-flagged contract edit
     per Phase 3 D-10(c) — never an inline relaxation.

  2. **04-11 must-have 4** — per-crate cargo-mutants baselines and an aggregate adjusted score.
     1 of 4 crates attempted, interrupted at ~68 min, no score produced. Human ruled this out
     of the phase sequence and into a **standalone compute ticket** (≥10 h for 890 mutants is a
     compute-budget decision CLAUDE.md reserves for the human). **No mutation score exists for
     Phase 4 and none may be inferred.** Owner unassigned; successor to D-04-11-B.

  3. **WR-01** — the APR write path is not race-free. `fs::rename` in `atomic_write` replaces
     its destination unconditionally, so a file created between check and rename is destroyed
     without `--force`. 04-20 narrowed the window; closing it needs `O_CREAT|O_EXCL`.

  4. **SAFE-02 "in CI"** — the 16 setfit legs at `.github/workflows/ci.yml:378-397` (applied
     `57f7823ab`) have NEVER EXECUTED. All Phase 4 evidence is macOS/arm64 local. The arch
     asymmetry is two-way and only one direction has been measured: `make tier2` is known RED
     on arm64 with 24 clippy errors in arch-gated SIMD that X64-Linux CI never lints.

**Phase 03 is COMPLETE.** Its five `human_needed` items were adjudicated 2026-08-14 —
`03-HUMAN-UAT.md` is `status: complete`, `03-VERIFICATION.md` reads `passed`. The ROADMAP
progress table still showed `human_needed` for Phase 3 as of `b3f816c25`; that is table lag,
not an open item. The four Phase 3 code-review blockers below are retained as the historical
record of what was found and why it was not an implementation defect:

  - CR-01: Phase 3's ~2900 lines of unit tests and all seven trybuild cases run in NO tier and NO CI
    job. `setfit` is declared (aprender-train/Cargo.toml:79) but not default; no workspace member
    enables it; tier3's `cargo test --all` and CI's nextest both compile the module out. tier3 only
    `cargo check`s the feature (setfit-feature-matrix, Makefile:338) and runs 2 repro tests.

  - CR-02: `setfit_repro_recorded_matches_expected_replay` matches neither Makefile filter, so the
    one test separating "reproducible" from "correct" never runs — and libtest exits 0 on a
    zero-match filter, so both repro gates go vacuous on a rename while printing success.

  - CR-03: serde_json renders non-finite f64 as `null`, so UpdateEvidence::table_hash is not
    injective over inf/NaN; the contract precondition demanding a typed failure before hashing has
    no implementation.

  - CR-04: thresholds_match_the_contract checks entry COUNT then a SUBSET test, so widening
    CALIBRATED_REGIMES keeps it green while admitting uncalibrated runs.

**Two of 03-10's must_haves were unmet at the time of that verification, both blocked on the
same compute-budget decision CLAUDE.md reserves for the human. They were adjudicated in
`03-HUMAN-UAT.md` on 2026-08-14 — and Phase 4's mutation deferral (item 2 above) is the same
shape, now routed to a standalone compute ticket:**

  1. Scoped cargo-mutants adjusted score >= 85% — NOT RUN. 1181 mutants inventoried;
     ~44.6 h projected single-job. Two measured tooling blockers: `--in-place` conflicts
     with `--jobs` in cargo-mutants 25.3.1, and the plan's mandated `--timeout 20` kills
     the BASELINE (aprender-core's 14285-test binary has not finished LINKING in 20 s;
     `elapsed=20.050001083s -> Timeout`). No score is claimed.

  2. `make coverage` — deferred; needs the same uncontended target dir.

TRN-07 is deliberately left unchecked: its compile-time negatives landed, but no
out-of-crate or `apr` path exercises create_selection_lock -> mint_test_token -> grant,
so nothing demonstrates a user REACHING the lock.

REPAIRED BY HAND after the GSD state/roadmap handlers (recurring defect — 5th occurrence at
Phase 3, and the first where the damage was a FALSE COMPLETION CLAIM. **It recurred a 6th time
at Phase 4**: `roadmap.update-plan-progress` marked Phase 4 Complete on summary-count parity
while `04-VERIFICATION.md` still read `gaps_found`, and the two artifacts sat in contradiction
for 25 commits until the 2026-08-16 UAT reconciled them. The rule stands: diff STATE.md and
ROADMAP.md after EVERY handler call and revert any completion claim the verifier has not
earned; the handler's own `"complete": true` is not evidence of anything):

  - `state.begin-phase` wrote `Plan: 1 of 10` on a resume at plan 10, left `stopped_at` on
    the Phase 2 value, wrote the phase percentage (40) into a field the body renders as a
    plan percentage, and left Phase 2's `human_needed` sentence reading as if it described
    Phase 3.

  - `roadmap.update-plan-progress 03 03-10 complete` then marked the WHOLE PHASE complete
    (`[x] Phase 3 ... (completed 2026-08-12)`, Progress table -> Complete) purely because
    summary_count reached plan_count — before the verifier ran and with two must_haves
    unmet. Reverted to `[ ]` / "Awaiting verification".
Phase 2's outstanding items live in `02-HUMAN-UAT.md` (status: partial, 4 human decisions
incl. the publish cascade) and are unchanged by these repairs.

Working branch: `gsd/phase-2-contract-gate` @ b3f816c25. Phases 2, 3 and 4 all ride this one
branch — see 02-01-SUMMARY.md for the branch/PR policy. **No PR has been opened yet; per
02-01 that is the human's call after the verifier runs.** This is now also why SAFE-02's
"in CI" clause is unproven: with no PR, the 16 setfit legs in ci.yml have never executed.

Progress: [████████████████████] 100% (50 of 50 PLANNED plans executed across phases 1-4;
Phase 5 is not yet planned, so this is NOT milestone completion. Phase 4 is executed and
UAT-passed but NOT CLOSED — `/gsd:secure-phase 04` has not run, and SC1/SC3 remain unmet
pending F-10 in Phase 5.)

## Performance Metrics

**Velocity:**

- Total plans completed: 9 (this milestone's execution log; Phase 1 predates metric capture)
- Average duration: ~1h40m
- Total execution time: ~14.8 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| Phase 02 P01 | 1 | 1h20m | 1h20m |
| Phase 02 P02 | 1 | ~35m | ~35m |

**Recent Trend:**

- Last 5 plans: 02-05 (~2h10m, 3 tasks, 15 files), 02-06 (~1h50m, 3 tasks, 6 files), 02-07 (~2h45m, 3 tasks, 11 files), 02-08 (~1h45m, 3 tasks, 15 files), 02-09 (~2h45m, 3 tasks, 6 files)
- Trend: the long plans are the ones that had to derive evidence independently — a second implementation, an induced mutation, a scaling measurement — rather than assert it. 02-09 matches 02-07's length for a different reason: it is the first plan whose evidence is a real CLI run against live pinned data rather than a test, and roughly 20 minutes of it was an ENOSPC stop-and-report (the phase's second; both were `target/debug/incremental` at ~25 GB)

*Updated after each plan completion*
| Phase 02 P03 | 55m | 3 tasks | 6 files |
| Phase 02 P04 | ~50m | 2 tasks | 15 files |
| Phase 02 P05 | ~2h10m | 3 tasks | 15 files |
| Phase 02 P06 | ~1h50m | 3 tasks | 6 files |
| Phase 02 P07 | ~2h45m | 3 tasks | 11 files |
| Phase 02 P08 | ~1h45m | 3 tasks | 15 files |
| Phase 02 P09 | ~2h45m | 3 tasks | 6 files |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 06 P01 | 37 min | 2 tasks | 40 files |
| Phase 06 P02 | 20 min | 3 tasks | 4 files |
| Phase 06 P03 | 24 min | 2 tasks | 3 files |
| Phase 06 P04 | 23 min | 3 tasks | 4 files |
| Phase 06 P05 | 46 min | 4 tasks | 13 files |
| Phase 06 P06 | 49 min | 3 tasks | 8 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: Use five hard capability gates in dependency order; later phases retain earlier invariants as regression contracts.
- [Phase 1]: Use `aprender-core::autograd::Tensor` as the only SetFit graph and prove the pinned MiniLM path before exposing training.
- [Phase 3]: SetFit identity requires encoder-update evidence followed by a separate unique-row multinomial head fit.
- [Phase 4]: Only a closed, production-reloaded, parity-verified F32 APR may reach evaluation, CLI prediction, benchmarking, or serving.
- [Phase 5]: Claims require all 40 shot/seed cells and identical sampled IDs for SetFit and 9B LoRA.
- [Phase 2]: Phase 2 ships as exactly two PRs — the D-06 baseline PR (#1, stacked on the Phase 1 branch) and one Phase 2 PR from gsd/phase-2-contract-gate after wave 6; plans 02-02..02-09 commit to that single branch and open no PRs of their own.
- [Phase 2]: An as-is baseline landing is attested by SHA-256 recorded before staging and re-verified against the committed blobs via git cat-file — git status alone cannot prove byte-identity for an untracked file.
- [Phase 2]: Declared Kani harnesses must state in-contract that they are not executed and name an identically bounded runnable proptest; cargo-kani is absent repo-wide, including for Phase 1's setfit contract.
- [Phase 2]: $(CONTRACTS) in the Makefile is an explicit list, not a glob — a contract file in contracts/ is validated by nothing until it is appended there.
- [Phase 2]: `git status --porcelain` is rewritten by the rtk hook and prints "ok" on a clean path, so every porcelain-emptiness assertion in this phase must run through `rtk proxy`.
- [Phase 2]: cargo package -p apr-cli is KNOWN-RED from wave 2 until the publish cascade — and so is --no-verify; --no-verify skips the packaged-crate BUILD, not the manifest resolution that rewrites the path dep into a registry dep. Control-verified: removing the dep line makes the identical command exit 0 with 581 files.
- [Phase 2]: Both CB-510 guard scripts pass VACUOUSLY on macOS — they use GNU grep -P, BSD grep exits 2, the trailing || true swallows it, and they report 0 include!() files where the true count is 1768. Logged as D-ITEM-01; compensating direct evidence taken for the new crate.
- [Phase 2]: The binding registry ACCEPTS module_path: aprender_contrastive_data::* under target_crate: aprender (bound 0->1, BIND-001 24->23, no namespace complaint), so plan 02-08 proceeds as written. Traps: contract: must be the BARE filename (a ../ prefix parses and binds nothing) and status: accepts only implemented|partial|not_implemented|pending.
- [Phase 2]: D-04 is enforced by two POSITIVE checks — a dependency allowlist compared against the resolved cargo tree closure, and a src/-wide fs/net/path symbol ban with NO cfg(test) exemption. All four failure modes were induced, observed and reverted before the gate was trusted.
- [Phase 02]: 02-03: DatasetProfile carries an associated type Splits, so PreparedDataset<Compatibility> has no validation field at all rather than an Option+expect() — Makes D-19 structural rather than a runtime invariant, and makes 02-08's trybuild non-constructibility gate provable: proving a field is always None needs whole-program reasoning, proving it does not exist is a type error.
- [Phase 02]: 02-03: SplitFingerprintInput ordering is discharged by BTreeMap iteration order via Split::exact_hash_pairs(), not a caller-side sort — An ordering obligation left to callers can be silently omitted, and a wrong order yields a plausible-looking wrong digest. There is now no unsorted path to construct the input from.
- [Phase 02]: 02-04: reference fixtures record MEASURED pinned-setfit behavior in one family and Aprender's contracted closed forms in another; every number must agree three ways (measurement, closed form read out of sampler.py, contract literal) or the generator aborts
- [Phase 02]: 02-04: fixtures are keyed by fixture_id rather than by class layout, because 8_4_8 and 8_4_8_maxpairs100 share [8,4,8] and a layout-keyed map would silently drop one
- [Phase 02]: 02-05: permutation invariance is claimed for the SELECTION, not the semantic hash — the payload embeds a dataset fingerprint that digests each split's JSONL in ingest order, so a permuted file is legitimately a different dataset; the test asserts both halves
- [Phase 02]: 02-05: the selection AccessRecord carries the DATASET fingerprint, not the validation-split digest the plan named, so one ledger field does not mean two things depending on which code path wrote it; D-19 evidence is discharged via validation_witness().dataset_fingerprint_hex() and profile
- [Phase 02]: 02-05: RNG byte-encoding and ordered-selection goldens are ALGORITHM-DERIVED (independent Python from the contract text, cross-checked with shasum); the four payload.json goldens are capture-and-blessed byte forms, and the SUMMARY labels which is which
- [Phase 02]: 02-06: from_attested_bytes routes through Split::from_jsonl_bytes rather than re-using from_labeled_rows, which required extracting a pub(crate) from_validated_splits in prepared.rs — the plan forbade touching prepared.rs, but that constraint's stated reason (wave-4 parallelism with 02-05) had already expired, and without the change the attested path would not have gone through the byte-ingest door and the cross-path fingerprint test would have been a tautology
- [Phase 02]: 02-06: the setfit compatibility profile's merged rows now carry source_split compatibility_test instead of validation/test — an unavoidable consequence of D-19's distinct role plus the crate's role gate, declared by the schema_version 1->2 bump and pinned by a test; canonical row bytes are unchanged and proven byte-for-byte
- [Phase 02]: 02-06: write_outputs re-opens the directory it just wrote through PreparedDataset::from_attested_bytes and rolls back on rejection — without a production caller the attestation read path and the schema-version gate would have been test-only dead code
- [Phase 02]: 02-06: ContrastiveDataError gains UnsupportedNormalizationVersion (a String tag cannot ride in UnsupportedSchemaVersion's u32), with OBLIG-CPP-ERROR-TAXONOMY extended per the process error.rs itself mandates
- [Phase 02]: 02-06: every crate-level test command in the tweet-eval contract carries --lib, because the bare filter form emits a 'test result: ok' line from a suite that ran ZERO matching tests and would satisfy the expected_output grep vacuously
- [Phase 02]: 02-07: PairLayout is a separate PUBLIC type holding the sampler's whole retained state, because a Selection always carries shots_per_class in EVERY class and therefore cannot express the K = N all-singleton layout DATA-05 must survive — without it plan 02-08's headline capacity gate would be unwritable from outside the crate
- [Phase 02]: 02-07: negative_capacity accumulates the running prefix rather than (S^2 - sum n^2)/2 — same O(K), same quantity, but it overflows exactly when the RESULT does; a test pins the two derivations against each other wherever the second is computable
- [Phase 02]: 02-07: NoPairCapacity is checked BEFORE the default budget resolves, following the degenerate equation's invariant prose rather than its formula line, which would have reported ZeroBudget for a layout with no pair space at all
- [Phase 02]: 02-07: the pair manifest hash refuses a record whose selection_hash or budget does not describe the sampler it is handed; hashing stream X under record Y would be the exact failure the header-inside-the-digest design exists to prevent
- [Phase 02]: 02-07: both untrusted_pair_ingest and split_span_fail_closed are STACKED on the public validate_pair_records — the contract macro accepts two attributes (verified by compiling both forms), and a binding that names a private helper is harder to audit
- [Phase 02]: 02-07: the GSD SDK state handlers damaged STATE.md more severely than env note 7 records — state.update-progress reported percent 89 while writing 20 into the frontmatter and leaving the body bar at 83, state.record-metric flipped status to 'completed' mid-phase, state.advance-plan clobbered last_activity to a bare date and left stopped_at on the previous plan, and add-decision tagged all five entries [Phase ?]. Every field was repaired by hand and read back. Plans 02-08/02-09: run the handlers, then READ THE FILE and repair
- [Phase 02]: 02-08: the capacity bound is the contract's c*(examples+classes), not the plan's c*(examples+budget) — a bound containing the budget cannot express the same obligation's budget-independence clause and grows the allowance exactly when the pair space grows
- [Phase 02]: 02-08: the blocking tier3 binding gate is the SCOPED contract-audit-phase2; the repo-wide contract-audit prints 132 BIND-001 errors across 38 of 44 contracts and exits 0 anyway (its loop never reads the audit status) — logged as D-ITEM-04, not fixed
- [Phase 02]: 02-08: cargo-mutants ran at --timeout 20 rather than the planned 60, from a measured sample — 9 percent of mutants hang, and 48 hangs x 60 s alone exceeds the whole 2700 s wall budget; the full 529-mutant run then COMPLETED
- [Phase 02]: 02-08: 10 of 22 surviving mutants are individually justified as unobservable (disjoint-bit OR/XOR, union-by-size balancing, single-variant enums, an accessor no constructible Selection can make non-zero) and were re-run to confirm they still survive; 12 were killed and a targeted re-run reported 14/14 caught
- [Phase 02]: 02-08: official_f_avg binds to entrenar::eval::classification::metrics::f1_average_for_classes with NO #[contract] attribute added to aprender-train; the plan's ClassificationMetrics does not exist, the type is MultiClassMetrics
- [Phase 02]: 02-09: apr data select and apr data pairs are pure filesystem adapters — 0 occurrences of Sha256, swap(, unrank or json! in non-comment lines, exactly ONE fs::rename site, zero File::create, zero unwrap(); all semantics including the on-disk manifest envelope, budget resolution and the manifest->Selection path stay in the crate
- [Phase 02]: 02-09: the seed MODE is DERIVED from the recorded root_seed rather than stored beside it — SelectionPayload is crate-owned and deny_unknown_fields, adding a field would need a schema bump invalidating 02-05's goldens, and a stored mode could only ever disagree with the seed printed next to it
- [Phase 02]: 02-09: cli.offline is deliberately NOT threaded into either command — neither opens a socket (the crate cannot; make contrastive-data-boundary enforces it), so an offline switch would advertise a capability that does not exist
- [Phase 02]: 02-09: the split ROLE SET is read out of the attestation rather than hardcoded as a canonical triple, so a compatibility directory is refused by PROFILE instead of dying on a missing filename — a true statement about the wrong problem
- [Phase 02]: 02-09: atomic_write takes a FILL CLOSURE (atomic_write_with) rather than a byte slice, so --dump streams dump_pairs into the temp file instead of buffering up to ~60 MB; one rename site, so Task 2's three write-safety proofs still cover both artifacts
- [Phase 02]: 02-09: the plan's Task 3 replay rejections are unreachable from the CLI because SelectionManifest::from_bytes verifies the digest BEFORE returning — the tests reseal forged payloads with the crate's public hash::exact_hash, which reaches the membership, row-hash and recomputation rungs without naming Sha256 in apr-cli
- [Phase 02]: 02-09: Task 3 had NO executable RED (its tests could not compile until the interface existed, the same produced-before-consumed constraint the checker found in Task 1); its gates are falsified by three induced mutations instead, and the SUMMARY says so rather than manufacturing a RED after the fact
- [Phase 02]: 02-09: scripts/check_apr_bin_pinned.sh does NOT scan docs — measured with a two-sided control (a bare apr added to the doc is silent; a bare @apr Makefile recipe fires BARE-APR Makefile:1282), both reverted; nobody should later assume documentation is covered
- [Phase 03]: 03-10: the GSD tracking handlers damaged STATE.md/ROADMAP.md a FIFTH time, and this time the damage was a FALSE COMPLETION CLAIM, not a cosmetic field: `roadmap.update-plan-progress <phase> <plan> complete` marks the WHOLE PHASE `[x] (completed <date>)` and flips the Progress table to Complete as soon as summary_count == plan_count — before the verifier runs and regardless of unmet must_haves. `state.begin-phase` separately wrote `Plan: 1 of 10` on a resume at plan 10 and put the phase percentage in a field the body renders as a plan percentage. The orchestrator MUST diff both files after every handler call and revert any completion claim the verifier has not earned; reading the handler's own JSON (`"complete": true`) is not evidence of anything
- [Phase 02]: 02-09: the GSD state handlers corrupted STATE.md a THIRD time and in a NEW way — update-progress reported percent 100 while writing 40 into the frontmatter (and 94/20 on the earlier call: it writes the PHASE percentage into a field the body renders as a PLAN percentage), record-session silently ignored its positional stopped-at argument, record-metric REJECTS the documented positional form and needs --phase/--plan/--duration flags, and advance-plan clobbered last_activity to a bare date. Every field repaired by hand and read back
- [Phase 06]: FALSIFY-MONO-011 treats thin MCP deployment units as a SECOND, separately-ratcheted category (human decision `deployment-unit-class`, 06-02 Task 1) — Human answered the gate="blocking-human" checkpoint with "deployment-unit-class", no wording changes. `allowed_bins` and ALLOWLIST_BASELINE = 27 stay byte-identical so "migration debt" keeps meaning migration debt; a new `deployment_unit_bins` set of six names (the four SetFit crates plus aprender-mcp-forecast and the not-yet-created aprender-mcp-chronos) gets its own DEPLOYMENT_UNIT_BASELINE = 6 shrink-only assert and its own stale-entry check. Policy sentence: "publish = false thin MCP servers whose capability IS a protocol surface; adding one requires a CONTEXT decision, never a same-PR edit." The existing ratchet's claim that every entry is a capability awaiting `apr <subcommand>` migration stays true because CONTEXT explicitly defers `apr forecast`, so these six are not awaiting migration. Rejected: single-list-33 (blurs debt with deployment unit), phase6-only-29 (leaves the gate RED, SC5 unreachable), halt (contradicts D-06 and the `apr forecast` deferral).
- [Phase 06]: NeuralProphet-lite ships behind the one forecast tool with the D-10 training rules as invariant tests: graph-connected Huber (core's own smooth-L1 loss is detached and returns None gradients), strict mini-batches (full batch collapses the fit to MAE 2.6), clear_graph() per step, and lr selected by TRAIN loss — Selecting by test error would have reported 0.4112 instead of 0.4463 on this very dataset - a fitted curve presented as a forecast. The bars are one-sided thresholds rather than value-parity residuals because NeuralProphet's lr-finder and torch RNG are not reproducible from the committed oracle.
- [Phase 06]: Contract bars must be read with the contract name written IN FULL at each call site, never through a Rust constant — The acceptance criterion is a STATIC link check (grep -c). A Rust constant reads the right file at runtime but makes the link invisible to the grep, so the guard silently stops guarding. Fixed at 12 call sites in 06-04.
- [Phase 06]: D-13 memory clause AMENDED (blocking human decision `amend-memory-clause`): the shipped Bolt keeps BOTH weight layouts — dot8 needs contiguous [out, in] rows for D-14 single rows, gemm_blis needs the [in, out] transpose for multi-row. Cost asserted and printed: 69.18 MB resident vs 34.61 MB, exactly 2.00x. NOT the SC4 < 30 MB binary bar. — The frozen quantiles_abs_f32 = 1.0e-6 was measured under this routing; deleting the untransposed copies would move every single-row product onto gemv and change the float accumulation order, so the bar would stop being evidence. Re-confirmed in-tree at 9.5367e-7.
- [Phase 06]: The enforced Chronos f16 sha256 is f5dc2ef53533c8896bcb120a754c52c39d8917c15750a9e845192014dfa74a67, NOT the f9a033b4... RESEARCH A3 predicted: safetensors 0.8.0 orders the two __metadata__ keys differently than the spike-007 writer did. Same 101 tensors, byte-identical 17,305,344-byte body; only the header differs. — Re-pinning to the value this toolchain reproduces keeps the check fail-closed; loosening it to a warning would have deleted the property REVIEW-06-03 asked for. Both values are documented in the justfile, the crate README and contracts/chronos-bolt-parity-v1.yaml.
- [Phase 06]: The forecast tool boundary is pinned in ONE contract (contracts/forecast-tool-boundary-v1.yaml) that BOTH servers are held to; the six Rust bounds are asserted EQUAL to it at test time, proven by inducing RED from the YAML alone. — D-03/D-15: a bound written twice can be loosened in one place. Reading it from the contract makes the YAML the source and the constant the mirror, and a bound edit becomes a pv diff-visible change rather than an inline relaxation.
- [Phase 06]: pmcp 2.19.3 emits JSON-RPC -32603 (the INTERNAL error code) for Error::validation, so a refusal test pins BOTH the code and the "Validation error: " message prefix. — error_code() returns None for both the Validation and Internal variants, so the numeric code cannot distinguish a caller fault from a server fault. The thiserror-rendered prefix is the only discriminator the SDK ships. Read off a live reply, not assumed; breaking map_error turns 20 of 21 refusal cases red.
- [Phase 06]: REVIEW-06-04: the pool claim is split by instrument. Equality under load is a unit-test correctness claim in CI; the >= 2.0 speed-up is a benchmark claim owned by just forecast-pool-ratio (06-08). mod pool_equality contains ZERO timing assertions. — A wall-clock ratio under four Tokio workers and eight heterogeneous blocking fits moves with CPU throttling independently of the router serialisation the pool removes. A flaky bar in the correctness suite trains readers to ignore the suite, including the equality failure that would matter.
- [Phase 06]: The router pool is evidenced by a CONTROL pair, not a single number: POOL=1 measures 1.002x and POOL=8 measures 2.070x on the same host with the same eight requests, reproducing spike-010 in-tree. — CLAUDE.md Verification Discipline #2: never label a run by intent. Without the 1.002x control, 2.070x could have come from anything; with it, the pmcp router mutex is demonstrably what the pool removes. Equality held in BOTH configurations.

### Pending Todos

- [Phase 2 — CLOSED BY 02-09, the policy held]: DATA-01 through DATA-06 were deliberately left
  UNCHECKED after 02-01 and 02-02, because those plans shipped a crate of `//!`-doc stubs and
  checking the boxes would have put five false claims in the traceability table. The policy was
  to mark each at the plan that actually closes it, and that is what happened: 02-06 closed
  DATA-01/02, 02-05 closed DATA-03, 02-07 closed DATA-04/05, 02-08 closed DATA-06. **02-09
  re-audited all six against the shipped behaviour before letting the table stand**, and each is
  genuinely delivered at the "a user can…" tier this milestone demands — every one was
  demonstrated in 02-09 by a real `apr` run against the live pinned TweetEval revision
  (587/66/280 with provenance; ten contracted seeds each replaying its own hash; a 256-pair dump
  audited endpoint-by-endpoint against the splits; a fixed budget holding while examples grow 8x;
  and the compatibility, mixed, forged and stale directories each refused fail-closed on BOTH
  commands). `requirements mark-complete DATA-03 DATA-04 DATA-05` returned
  `updated: false, already_complete` and REQUIREMENTS.md is byte-unchanged. **Nothing was closed
  to make the table look finished, and nothing is left open.**

- [Phase 2 — PARTIALLY CLOSED BY 02-08]: `official_f_avg` and all 24
  contrastive-pair-protocol equations are now BOUND, and `make contract-audit-phase2` (blocking,
  in tier3) keeps them bound. The REPO-WIDE `make contract-audit` is not fixed and is worse than
  this entry recorded: measured, it reports **132 BIND-001 errors across 38 of the 44 contracts**
  — 10 of them Phase 1's setfit equations — and **exits 0 anyway**, because its loop body never
  reads the audit's status. It is a target that prints failures and reports success. Logged as
  D-ITEM-04 in the phase's `deferred-items.md`; the honest fix is to make it read its status and
  then either bind the 132 or mark them `status: pending` (a BIND-004 warning, not a BIND-001
  error). Still worth a dedicated binding-registry pass.

- [Repo-wide]: No `#[kani::proof]` harness exists anywhere in `crates/` and `cargo-kani` is not
  installed, yet contracts declare harnesses. 02-01's and 02-02's contracts now say so explicitly
  in-prose and name their runnable proptest backing; Phase 1's
  `setfit-encoder-conformance-v1.yaml` still does not.

- [Repo-wide]: Both CB-510 packaging guards (`scripts/check_include_files.sh`,
  `scripts/check_package_includes.sh`) pass VACUOUSLY on macOS. They use GNU `grep -oP`; BSD grep
  exits 2 with `invalid option -- P`, the trailing `|| true` swallows it, and both print
  "All 0 include!() files" and exit 0. True count via `ggrep`: 1768. CI runs on Linux so this is a
  local false-green, but `make tier3` tells a developer something untrue. Surfaced by 02-02 as
  D-ITEM-01 in the phase's `deferred-items.md`; fix is a repo-wide shell-portability change with
  its own must-match/must-not-match case table (CLAUDE.md rule 7). Worth a dedicated ticket.

- [Repo-wide — SURFACED BY 02-09]: `scripts/check_apr_bin_pinned.sh` does **not** scan
  documentation. Measured with a two-sided control rather than read: a bare `apr data select …`
  added to `docs/examples/tweet-eval-stance.md` leaves it green (rc=0, 28 files), while a bare
  `@apr qa model.apr` added as a Makefile recipe fires `BARE-APR Makefile:1282`. Both probes
  reverted. This is the guard's stated design ("the invariant is what CI executes, not that
  nobody may ever type apr"), so it is a scope note rather than a defect — but a reader who
  assumes docs are covered will be wrong, and the docs are where a user copies commands from.
  Worth deciding deliberately whether user-facing docs should be in scope.

### Blockers/Concerns

- [Phase 1]: Freeze numerical tolerances from pinned reference fixtures before examining Rust discrepancies; validate the real-weight mixed-batch graph before committing the full BERT refactor.
- [Host — RECURRING, hit TWICE in this phase]: `target/debug/incremental` regrows to ~25 GB and
  fills the volume; 02-09 stopped mid-plan with `ld: write() failed, errno=28` at 546 MiB free.
  Nothing was deleted by the executor (standing instruction) and the run was reported as a
  checkpoint with measured numbers; the coordinator reclaimed ~21 GB. **Mitigation adopted for
  the rest of the plan and recommended for Phase 3: `export CARGO_INCREMENTAL=0`** — the cache
  did not regrow and 15 GB was still free at plan end.
- [Host — THIRD occurrence, 2026-08-17, BLOCKING Phase 05 wave 2]: the volume is 100% full —
  **791 MiB free of 926 GiB**. `target/` is 138 GB, of which `target/debug/incremental` is
  **90 GB** (~3.6x the ~25 GB recorded above), `target/debug/deps` 22 GB and
  `target/llvm-cov-target` 19 GB. Measured by the orchestrator, not inferred: `du -sh target`
  and `df -h /System/Volumes/Data`. Both wave-2 plans failed on it — 05-05 could not even be
  dispatched (`git worktree add` died with `No space left on device`), and 05-03's executor
  halted before Task 1 (see `05-03-SUMMARY.md`, `status: halted`). `CARGO_INCREMENTAL=0` was
  NOT in effect, so the mitigation adopted above has lapsed.
  **The orchestrator deleted nothing** — the user declined reclamation when asked. It was
  nevertheless reclaimed from outside this session at 12:05 local: `target/debug` (117 GB),
  `target/llvm-cov-target` (19 GB) and `target/coverage` were removed, leaving `target/` at
  1.7 GB and **138 GiB free**, so the `MIN_FREE_GIB = 10` preflight now clears with wide margin
  and wave 2 was re-dispatched on that basis. Note the reclaim also took `target/release`'s
  siblings but left `target/release` itself, so release builds are warm-ish, not cold.
  **`CARGO_INCREMENTAL=0` is still NOT exported** — the mitigation adopted after occurrence #2
  has lapsed, which is why the cache reached 90 GB. Re-export it before the next long build or
  this recurs a fourth time.

- [Phase 2]: Decide and version singleton-class and bounded-oversampling behavior during phase planning.
- [Phase 2 — KNOWN-RED, EXPECTED, NOT A REGRESSION — **WIDENED BY MEASUREMENT IN 02-02**]: `pre-release` Gate 5 fails from Phase 2 wave 2 through phase exit. Cause: `apr-cli` gains a dependency on the new `aprender-contrastive-data` crate, which is not on crates.io until the human-approved publish cascade lands it (RESEARCH Pitfall 8 / Finding F5). **CORRECTION (02-02, measured):** it is NOT only the verifying form. `cargo package --no-verify -p apr-cli` ALSO fails — `--no-verify` skips the packaged-crate BUILD, not the MANIFEST RESOLUTION that rewrites the path dep into a registry dep, and resolution is where it breaks (`no matching package named 'aprender-contrastive-data' found`). Control-verified: with the dependency line temporarily removed the identical command exits 0 and packages 581 files. So ANY `cargo package -p apr-cli`, verifying or not, is red. What IS gated and must stay green: `cargo package --no-verify -p aprender-contrastive-data` (rc=0, 19 files). Exit condition unchanged: publish `aprender-contrastive-data` BEFORE `apr-cli` — a human-approved release action; CLAUDE.md forbids self-serving the publish. `/gsd:verify-work` must read a red Gate 5 as this expected state. Mirrored in `must_haves.caveats` of plans 02-02 and 02-08 and in 02-VALIDATION.md; plan 02-08's acceptance criterion "both `cargo package --no-verify` runs exit 0" is falsified and should be read as the crate-only form.
- [Cross-cutting — HOST-SPECIFIC, PRE-EXISTING, NOT A REGRESSION]: `cargo check --workspace` cannot exit 0 on Darwin. `crates/aprender-profile/src/main.rs:6` is a `compile_error!("renacer requires Linux (ptrace syscall tracing)")` under `#[cfg(not(target_os = "linux"))]`, and the consequent E0601 (`main` not found) is its shadow rather than a second defect. Control measured at 02-08: `cargo check --workspace --exclude aprender-profile` exits **0**. Any plan whose acceptance criterion names a bare `cargo check --workspace` should be read as the `--exclude aprender-profile` form on this host.
- [Phase 5]: Choose validation-only calibration and uncertainty estimators before collecting benchmark results.
- [Cross-cutting]: Preserve CPU-only package/MSRV/feature combinations and executable contract conventions from the repository's pre-release and APR dogfood skills.
- make tier2 is RED on arm64: pre-existing clippy errors across 5 crates untouched by phase 2. Re-measured at 02-08: **24 errors, 44 locations** — aprender-compute 38, zram-core 3, present-terminal 1, core 1, serve 1; **zero in aprender-contrastive-data**, whose only appearance in the tier2 log is its `Checking` line. All arch-gated SIMD; CI runs X64-Linux-only so these aarch64-live arms are never linted. Proven independent of 02-03. See deferred-items D-ITEM-02.
- contracts/chronos-bolt-parity-v1.yaml quantiles_abs_f32_nonaarch64 = 5.0e-6 is PROVISIONAL AND UNMEASURED: no x86_64 run has happened, and every CI job here is [self-hosted, X64]. FALSIFY-CHRONOS-002 obliges the first x86_64 run to record its measured max|delta| and tighten the bar in a pv diff-visible edit.

## Deferred Items

Items acknowledged and carried forward from project scope:

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| Encoder/objectives | Additional encoder families and contrastive losses | v2 | Project definition |
| Optimization | Accelerator support and quantization beyond the CPU/F32 lifecycle | v2 | Project definition |
| Tasks | Multilabel, hierarchical, explanation, and persistent-cache workflows | v2 | Project definition |

## Session Continuity

Last session: 2026-09-06T14:39:32.484Z
Stopped at: Completed 06-06-PLAN.md
Resume file: None
