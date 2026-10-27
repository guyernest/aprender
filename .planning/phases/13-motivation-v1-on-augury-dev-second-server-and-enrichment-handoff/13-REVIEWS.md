---
phase: 13
reviewers: [codex]
reviewer_instances: [fable]
reviewed_at: 2026-10-27T21:23:02Z
plans_reviewed: [13-01-PLAN.md, 13-02-PLAN.md, 13-03-PLAN.md, 13-04-PLAN.md, 13-05-PLAN.md, 13-06-PLAN.md, 13-07-PLAN.md, 13-08-PLAN.md, 13-09-PLAN.md, 13-10-PLAN.md, 13-11-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review — Phase 13

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Codex Review

Read-only review completed for all 11 plans against the repository and partner references. I did not run tests, deployments, AWS/pmcp commands, or write files. The three existing v1 data files match the supplied SHA-256 pins. Proposed mechanisms below remain untested until implemented.

## 13-01

**Summary:** A well-scoped region change with strong compatibility checks. The renderer already performs substitution before writing, which supports the planned unchanged-output refusal cases.

**Strengths**

- The zero/two-region-line cases exercise the actual renderer boundary: the count check precedes both TOML parsing and output writing. [justfile:1760](/Users/guy/Development/machine-learning/aprender/justfile:1760)
- Positional forwarding addresses a real quoting defect: `laya-prepare-motivation` currently pastes `{{args}}` directly into its shell script. [justfile:1340](/Users/guy/Development/machine-learning/aprender/justfile:1340)
- Comparing against the pre-phase renderer provides stronger compatibility evidence than comparing two invocations of the newly modified implementation.

**Concerns**

- **LOW:** The live prohibition on `LAYA_DEPLOY_TEMPLATE` lacks an explicit behavioral test. The region selftest always enables `DRY_RUN`, so it cannot establish that the override is refused before a live AWS call.
- **LOW:** The evidence’s `region_commit` names the Task 2 commit, although the implementation spans two commits. This is a usable final revision, but should be described that way rather than implying one atomic region change.

**Suggestions**

- Add a live-mode override-refusal case with an AWS recorder, requiring refusal and zero recorder calls.
- Record both implementation commits, or explicitly identify `region_commit` as the final tested revision.

**Risk Assessment: LOW.** The principal render and argument-handling risks are addressed; the remaining gap is verification of the live override restriction.

## 13-02

**Summary:** Declaring the formats before implementation is sound. However, acceptance by `pv` must be distinguished from validation of the formats’ substantive constraints.

**Strengths**

- `kind: schema` is an appropriate supported home for a reference format. Its metadata-only validation behavior is explicitly documented in the implementation. [kind.rs:40](/Users/guy/Development/machine-learning/aprender/crates/aprender-contracts/src/schema/kind.rs:40)
- Derived graph freshness is a real executable check: extraction compares both tracked graph files against freshly generated content and exits nonzero on drift. [extract_rdf.rs:79](/Users/guy/Development/machine-learning/aprender/crates/aprender-contracts-cli/src/commands/extract_rdf.rs:79)
- Leaving the README count unchanged is justified: its test explicitly permits understatements. [readme_contract.rs:188](/Users/guy/Development/machine-learning/aprender/crates/aprender-core/tests/readme_contract.rs:188)

**Concerns**

- **MEDIUM:** The proposed mirror checks cover keys, enums and mappings, but do not establish equivalence for all declared constraints—patterns, caps, probability tolerance, timestamp syntax and deployment restrictions. `pv` will not supply that missing semantic validation.
- **MEDIUM:** Adding evidence level `none` and refusing `other + clear` expands the locked three-value annotation convention. The guide may justify the representation, but these remain annotation-interface decisions that should be recorded explicitly. [13-02-PLAN.md:135](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-02-PLAN.md:135)

**Suggestions**

- Represent machine-enforced bounds structurally and include them in mirror checks and boundary tests.
- Record an explicit CONTEXT amendment explaining `none`, “evidence for the recorded function,” and the incoherent-pair refusals before the partner builds its sheet.

**Risk Assessment: MEDIUM.** Corpus integration is well specified; semantic drift between declaration, validator and annotation practice remains possible.

## 13-03

**Summary:** The merge architecture correctly preserves the existing split and keeps enrichment out of evaluation. Two test-contract inconsistencies should be corrected before execution.

**Strengths**

- Merging after the unchanged base build protects the actual split mechanism: eval membership currently depends on permutations over the ordered retained groups. [prepare_motivation.py:174](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/prepare_motivation.py:174)
- Keeping provenance outside training rows respects the existing loader’s closed `text`/`label` schema. [data.py:190](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/data.py:190)
- Reusing `write_once` preserves established refusal behavior for changed and foreign files. [prepare_stance.py:161](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/prepare_stance.py:161)

**Concerns**

- **MEDIUM:** “Manifests differ only in annotation-mode counts” is incompatible with recording each source file’s byte hash. Changing the header’s annotation mode necessarily changes that hash, and potentially the filename. Both requirements appear in the plan. [13-03-PLAN.md:143](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-03-PLAN.md:143), [13-03-PLAN.md:196](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-03-PLAN.md:196)
- **MEDIUM:** Direct reuse of `refuse_unencodable(..., "enrichment", ...)` introduces `enrichment-row-encoding`, outside the declared 17 refusal names. Its message also contains diagnostic details beyond the proposed refusal convention. [data.py:161](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/data.py:161)
- **LOW:** Printing source basenames assumes filenames cannot contain post fragments. The sentinel tests plant content in fields, not filenames, so they do not prove the stated universal no-text claim.

**Suggestions**

- Compare train bytes and semantic manifest fields; explicitly allow source hashes and source identifiers to differ.
- Translate encoding failures into the declared `enrichment-encoding` refusal.
- Use generated source identifiers in printed output, or constrain and test filenames.

**Risk Assessment: MEDIUM.** The split and conflict design is strong; the identified issues concern executable acceptance and output guarantees.

## 13-04

**Summary:** This plan addresses a genuine state-collision hazard, but its target guard does not yet enforce the full deployment identity. Its log-restoration change also breaks an existing evidence consumer.

**Strengths**

- Per-profile state directly addresses destructive existing behavior: deployment removes the id-keyed snapshot directory, and verification later reads that same id-keyed state. [justfile:2042](/Users/guy/Development/machine-learning/aprender/justfile:2042), [justfile:3021](/Users/guy/Development/machine-learning/aprender/justfile:3021)
- Checking endpoint ownership before verification’s AWS writes is valuable because verification deliberately changes the named Lambda’s environment. [justfile:3054](/Users/guy/Development/machine-learning/aprender/justfile:3054)

**Concerns**

- **HIGH:** `PMCP_API_URL` is omitted from the guard and environment cleanup. The actual resolver gives this variable precedence over the selected target’s API URL and preserves environment-sourced values. Thus `PMCP_TARGET=augury-dev` can coexist with a different control-plane destination. [resolver.rs:250](/Users/guy/Development/mcp/sdk/rust-mcp-sdk/cargo-pmcp/src/commands/configure/resolver.rs:250), [resolver.rs:378](/Users/guy/Development/mcp/sdk/rust-mcp-sdk/cargo-pmcp/src/commands/configure/resolver.rs:378)
- **HIGH:** The committed table lacks expected AWS account IDs. Current recipes derive bucket identity from whichever account STS returns; they do not independently enforce the intended account. A correctly named profile configured for another account can pass the proposed offline checks. [justfile:2028](/Users/guy/Development/machine-learning/aprender/justfile:2028)
- **HIGH:** Restoring eligibility logs before the selftest exits changes an existing producer-consumer contract. `armed_deploy_selftest()` copies the eligibility log **after** the child returns, and `row_verify()` requires that log to contain the newly tested artifact’s successful verification. It would instead receive the restored historical log—or no log. [justfile:3438](/Users/guy/Development/machine-learning/aprender/justfile:3438), [justfile:3609](/Users/guy/Development/machine-learning/aprender/justfile:3609)
- **MEDIUM:** A wiring check that merely finds `_laya-target-check` in a recipe cannot prove it executes before the first AWS call.

**Suggestions**

- Validate/export the expected API URL and reject conflicting `PMCP_API_URL`.
- Add account IDs to the deployment table and perform a live STS identity check before mutations.
- Export successful selftest evidence into its scratch directory before restoring historical files; update gate consumers to read that result.
- Add a test that moves the target check after an AWS operation and must fail.

**Risk Assessment: HIGH.** These gaps affect wrong-tenant prevention and the required full gate sweep.

## 13-05

**Summary:** The selection rule is concrete and reproducible at the selected-row level. Its whole-file determinism requirement conflicts with provenance, and its prose-validation check is weaker than claimed.

**Strengths**

- Dedupe and exclusion reuse the repository’s existing normalized identity instead of introducing another normalization rule. [data.py:76](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/data.py:76)
- Control-first selection, explicit shortfalls and no backfill make sampling behavior inspectable and avoid silently changing quotas. [13-05-PLAN.md:99](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md:99)

**Concerns**

- **MEDIUM:** Shuffled input cannot produce a byte-identical complete selection file while its header includes the raw input file SHA-256. Shuffling changes that hash even when every selected row is identical. [13-05-PLAN.md:87](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md:87), [13-05-PLAN.md:112](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md:112)
- **MEDIUM:** The note check does not compare “every number” with implementation constants. It checks budget and seed, then merely searches for numeric strings. Incorrect shares, associations or formulas can pass. [13-05-PLAN.md:180](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md:180)
- **MEDIUM:** “Unbiased reference” needs a defined population. Exclusion and deduplication make it a reference for the eligible unique-text pool. It also cannot serve as an independent evaluation of a model subsequently trained on those same control rows. [13-05-PLAN.md:104](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md:104)

**Suggestions**

- Require identical selected rows and counts under shuffling, while allowing provenance hashes to differ.
- Generate the numerical rule section from constants or compare a structured declaration.
- Define the control’s population and limit its claims to the current scoring/annotation round.

**Risk Assessment: MEDIUM.** The algorithm is reasonable; its acceptance and statistical claims need narrowing.

## 13-06

**Summary:** Reusing the existing bearer path is the right approach. Credential transport and partner compatibility need additional controls before this can support a production hand-off.

**Strengths**

- The existing probe really does read the environment token and attach it as a bearer header; no Rust transport change is necessary. [examples/probe.rs:182](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-decide-lambda/examples/probe.rs:182), [src/probe.rs:193](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-decide-lambda/src/probe.rs:193)
- Testing refusal on tools rather than initialize matches the partner’s existing no-auth probe logic. [smoke.sh:290](/Users/guy/projects/mcp/motivation-experiment/scripts/smoke.sh:290)

**Concerns**

- **HIGH:** Treating Basic-only minting as successful acceptance does not establish compatibility with the partner. Its production client sends form-body credentials only, and its smoke script deliberately fails when only Basic works. [auth.rs:52](/Users/guy/projects/mcp/motivation-experiment/crates/core/src/auth.rs:52), [smoke.sh:256](/Users/guy/projects/mcp/motivation-experiment/scripts/smoke.sh:256)
- **HIGH:** Initial token-host validation is insufficient without an explicit redirect policy. The installed Python redirect handler follows POST 301/302/303 responses and copies headers except content headers; a Basic `Authorization` header can therefore cross to another destination. The planned helper does not prohibit this. [urllib/request.py:632](/Library/Frameworks/Python.framework/Versions/3.14/lib/python3.14/urllib/request.py:632), [urllib/request.py:647](/Library/Frameworks/Python.framework/Versions/3.14/lib/python3.14/urllib/request.py:647)
- **MEDIUM:** Deploy/verify re-exec is tested through dry-run messages and source matching, not a successful execution through those actual branches. That leaves argument preservation and token propagation through re-exec unproven. [13-06-PLAN.md:173](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-06-PLAN.md:173)
- **LOW:** `mode & 0o077 == 0` accepts more than modes 400/600, including 700. This differs from the stated contract. [13-06-PLAN.md:114](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-06-PLAN.md:114)

**Suggestions**

- Keep Basic fallback diagnostic, but require form-body minting for partner-ready acceptance.
- Disable token-endpoint redirects and test cross-host and HTTPS-to-HTTP redirects.
- Exercise both re-exec branches with controlled command/AWS/probe shims.
- Open credentials safely and validate the opened descriptor’s ownership, type and exact mode.

**Risk Assessment: HIGH.** Credential destination and real consumer compatibility are release-critical.

## 13-07

**Summary:** The preparation sequence is sensible: finish verification and upload before consuming the short-lived pmcp session. Two remote-evidence cases need stronger distinctions.

**Strengths**

- Upload genuinely runs the Rust verifier before its first AWS operation. [justfile:2797](/Users/guy/Development/machine-learning/aprender/justfile:2797)
- Taking only the login checkpoint after preparation conserves the authentication window.
- The explicit “only NotFound permits bucket creation” rule improves on the current recipe, which attempts creation following any failed head-bucket call. [justfile:2754](/Users/guy/Development/machine-learning/aprender/justfile:2754)

**Concerns**

- **MEDIUM:** S3 size equality does not establish byte identity. The current uploader skips an existing same-size object without checking its checksum, and the plan repeats only that size check. A wrong same-size object would be accepted as uploaded, although later model loading should fail closed. [justfile:2825](/Users/guy/Development/machine-learning/aprender/justfile:2825)
- **MEDIUM:** The augury sibling baseline permits “or absent” without specifying which AWS errors establish absence. AccessDenied, expired credentials and transport failure must remain unknown rather than becoming an absent baseline. [13-07-PLAN.md:123](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-07-PLAN.md:123)

**Suggestions**

- Verify an independently computed remote checksum, or hash a downloaded object when checksum evidence is unavailable.
- Use explicit `present`, `absent` and `unreadable` states; only ResourceNotFound establishes absence.
- Ensure the preparation commands explicitly select live mode and clear conflicting control-plane overrides.

**Risk Assessment: MEDIUM.** Ordering is good; remote identity and baseline error classification remain incomplete.

## 13-08

**Summary:** The identity chain and owner checkpoints are detailed, but the exposure timer understates the possible public interval. Some required evidence cannot be copied directly from the current probe output.

**Strengths**

- Compile identity, returned artifact identity and Lambda configuration provide useful independent checks. Existing deployment already checks the compiled thin crate and then invokes the identity probe. [justfile:2070](/Users/guy/Development/machine-learning/aprender/justfile:2070), [justfile:2106](/Users/guy/Development/machine-learning/aprender/justfile:2106)
- Separate OAuth and M2M checkpoints let the owner close anonymous access before completing client setup.

**Concerns**

- **HIGH:** `publish_done_utc` is recorded after the entire deployment recipe returns. That recipe has already published before IAM inspection, health retries and identity checks. Consequently, first-401 minus recipe completion is **not an upper bound on total exposure after publication**. [justfile:2063](/Users/guy/Development/machine-learning/aprender/justfile:2063), [justfile:2077](/Users/guy/Development/machine-learning/aprender/justfile:2077), [13-08-PLAN.md:117](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-08-PLAN.md:117)
- **HIGH:** Extra identity, negative-control, readback and evidence work precedes the owner’s “enable NOW” instruction. There is also no bounded response when the owner never replies; the polling timeout begins only after their reply. [13-08-PLAN.md:130](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-08-PLAN.md:130)
- **MEDIUM:** The identity JSON omits `labels`, although the internal report contains them. The planned `identity_line.labels` cannot be copied from that emitted line. [examples/probe.rs:289](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-decide-lambda/examples/probe.rs:289)
- **MEDIUM:** A Cognito `LAYA_M2M_TOKEN_HOST` override is not persisted alongside the file-path note. A later invocation can therefore reject credentials that already passed acceptance.

**Suggestions**

- Record a conservative exposure start before publication, or instrument the actual publish boundary.
- Notify the prepared owner immediately and defer optional checks; define a maximum open interval and containment response.
- Record expected labels separately with the probe’s actual label-order checks.
- Persist the nonsecret token-host choice and verify subsequent commands reuse it.

**Risk Assessment: HIGH.** The endpoint may remain open longer than the evidence reports.

## 13-09

**Summary:** The replay and timing coverage is broad, but the acceptance verifier currently permits a failed required four-text call to be reported as a pass.

**Strengths**

- Cold measurement uses a real environment update followed by a first-request probe and matching CloudWatch evidence. [justfile:3051](/Users/guy/Development/machine-learning/aprender/justfile:3051), [justfile:3067](/Users/guy/Development/machine-learning/aprender/justfile:3067)
- Preserving unsuccessful samples and deriving cold status improves the usefulness of the existing evidence writer, which otherwise omits per-sample error fields. [justfile:3113](/Users/guy/Development/machine-learning/aprender/justfile:3113)
- The existing replay output exposes the functional checks the plan intends to verify. [examples/probe.rs:316](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-decide-lambda/examples/probe.rs:316)

**Concerns**

- **HIGH:** The verify block explicitly accepts a four-text token-budget refusal, then independently requires every requester test to say pass. This does not prove the stated successful batch-of-four requirement. [13-09-PLAN.md:263](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-PLAN.md:263)
- **MEDIUM:** `reverify-once` has no defined transition when the second verification also fails. Task 3 permits only a verified result or an explicit hold-serving decision. [13-09-PLAN.md:182](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-PLAN.md:182), [13-09-PLAN.md:206](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-PLAN.md:206)
- **MEDIUM:** The automated after-check trusts `unchanged: true` rather than comparing the recorded before/after Lambda fields itself. [13-09-PLAN.md:267](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-PLAN.md:267)

**Suggestions**

- Require a successful four-text call using a known fitting synthetic payload. Preserve any larger-payload refusal separately.
- Define a second-failure branch that requires contain or hold-serving.
- Derive unchanged status directly from before/after records.
- Distinguish the recipe’s 30-second verdict from the owner’s decision to accept or reject the deployment.

**Risk Assessment: HIGH.** The current verifier can overstate partner acceptance.

## 13-10

**Summary:** The closure process is unusually thorough, but it should validate hand-off semantics and continuing regression coverage, rather than relying heavily on record flags and string-presence checks.

**Strengths**

- Running graph freshness checks in a clean checkout addresses contamination from untracked trees while exercising the real extractor. [extract_rdf.rs:79](/Users/guy/Development/machine-learning/aprender/crates/aprender-contracts-cli/src/commands/extract_rdf.rs:79)
- Explicit unchanged-surface checks make the “no verifier, artifact, door or register changes” boundary reviewable.
- The extra enrichment manifest appears compatible with existing data readers: both training and Rust verification open named files rather than rejecting arbitrary directory contents. [train.py:748](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:748), [verify.rs:1403](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:1403)

**Concerns**

- **MEDIUM:** New fixture-only ingest, selection and credential tests receive no CI entry. Existing `laya-train-selftest` covers only metrics, data, gate and lifecycle; the CI fragment mechanism exists but is not used here. [justfile:1435](/Users/guy/Development/machine-learning/aprender/justfile:1435), [ci_run_explicit_test_commands.sh:10](/Users/guy/Development/machine-learning/aprender/scripts/ci_run_explicit_test_commands.sh:10)
- **MEDIUM:** The hand-off check verifies only selected values and string presence. It does not establish that every numerical claim, auth-method compatibility statement or degraded verification outcome matches its record.
- **MEDIUM:** The sweep is useful defense in depth, but cannot prove “no credential or post text.” It recognizes selected shapes and canaries; non-JWT tokens and arbitrary post fragments remain outside that detector. The current probe also prints HTTP error bodies verbatim. [src/probe.rs:63](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-decide-lambda/src/probe.rs:63)

**Suggestions**

- Wire the portable synthetic tests into CI without adding real-data or live-cloud requirements.
- Generate hand-off values from structured records and explicitly represent degraded outcomes.
- Describe the sweep’s limits accurately and minimize raw output entering records.
- Replace the deferred manifest-compatibility question with a bounded fixture check if practical.

**Risk Assessment: MEDIUM.** Closure is comprehensive, but its strongest claims exceed what some checks establish.

## 13-11

**Summary:** Taking and pinning the local baseline before deployment work is necessary. The archive provides useful integrity evidence; mtime should be supporting metadata rather than proof of history.

**Strengths**

- The protected files correspond to real writers: deployment replaces config/state/logs, and verification writes shared plan files. [justfile:2041](/Users/guy/Development/machine-learning/aprender/justfile:2041), [justfile:3015](/Users/guy/Development/machine-learning/aprender/justfile:3015)
- Hashing original paths allows later checks to detect changes to the operational record, while archived copies preserve recovery material. [13-11-PLAN.md:105](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-11-PLAN.md:105)

**Concerns**

- **MEDIUM:** An old mtime does not prove bytes predate the phase. Timestamp-preserving copies are already used throughout these recipes, including restoration. The plan’s “provably older” claim is therefore too strong. [justfile:2277](/Users/guy/Development/machine-learning/aprender/justfile:2277), [justfile:2280](/Users/guy/Development/machine-learning/aprender/justfile:2280)
- **MEDIUM:** The existing-archive branch checks archive/original consistency but does not first compare the manifest with an already committed baseline pin. It then rewrites baseline evidence, allowing a resumed execution to repin a changed manifest. [13-11-PLAN.md:97](/Users/guy/Development/machine-learning/aprender/.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-11-PLAN.md:97)
- **LOW:** Interrupted archive creation leaves a partial directory requiring owner intervention. This is safe but avoidable.

**Suggestions**

- Establish baseline ordering through an explicit prerequisite and exclusive execution; retain mtimes as supporting observations.
- If baseline evidence exists, require its existing pin to match and never silently replace it.
- Build the archive in a temporary sibling directory and rename it into place after validation.

**Risk Assessment: MEDIUM.** The hash baseline is valuable, but its historical and idempotency guarantees need tightening.

## Overall assessment

**Recommendation: revise before execution. Overall risk: HIGH.**

The plans cover the phase goals well and generally preserve scope. The most important fixes are:

1. Close target-selection bypasses, including `PMCP_API_URL` and expected-account enforcement.
2. Repair the eligibility-log producer/consumer contract before requiring the full gate sweep.
3. Require partner-compatible form-body minting and prohibit credential-bearing redirects.
4. Measure OAuth exposure from a conservative publication boundary and bound the open interval.
5. Require an actually successful four-text acceptance call.
6. Correct the provenance-versus-byte-identity contradictions.

Keep the split-preservation design, per-profile state isolation, owner-only authentication steps and synthetic-data boundary. Reduce repeated expensive verification where the same committed revision has already passed, while retaining focused mutation tests and final integration checks.

---

## Claude Review (fable)

# Phase 13 plan review (one-shot, read-only)

Verified against the working tree at `9a7b166d8` plus the uncommitted justfile region diff, the Phase 10/12 evidence, the partner runbook, and the local `models/decide` state. Every mechanism claim below was traced in source; where a claim could not be verified it is marked as open.

Two facts the plans inherit from RESEARCH are already stale and worth knowing before execution:

- The partner repo HEAD is now `d822f119c…`, not `0c46b68…` as RESEARCH says. The plans compare HEAD/status before and after rather than pinning a value, so nothing breaks, but the hand-off should not quote `0c46b68`.
- `~/.pmcp/credentials.toml` now expires at `2026-10-27T21:32:20Z` (the owner has logged in since RESEARCH recorded it as expired). The token gate re-reads it, so no plan change is needed.

---

## 13-01

**Summary.** Commits the owner's three-hunk region diff and adds an offline region selftest, positional-argument hardening and gate rows. The diff is exactly what the plan says (`git diff justfile`: 18+/7−, hunks at justfile:1718, 1746-1763, 2742-2770). The template has exactly one `region = "us-east-1"` line (`crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:54`). The pre-phase commit `bc52bf3d` is correct: it is the last commit touching the justfile (`git log -1 -- justfile`), so HEAD's justfile equals it. The design is sound and the mutation set is well matched to the cases.

**Strengths.**
- The injection finding is real: `laya-deploy-config` pastes `"{{region}}"` into the python argv (justfile:1746) and `laya-weights-bucket` sets `R="{{region}}"` (justfile:2748). `[positional-arguments]` is the right fix and is already the house pattern (`_laya-iam-check`, justfile:1793; `just --show _laya-iam-check` prints the attribute first, so the `grep '^\[positional-arguments\]'` check works).
- `laya-prepare-motivation *args` forwards `{{args}}` unquoted (justfile:1340); the partner CSV path has spaces, so the ARGS case is a genuine hazard, not theatre.
- The gates drift check will indeed demand rows: a recipe printing `REFUSED`/`OK` is a gate (justfile:3262-3274) and a `not-a-gate` row is refused when the recipe prints a verdict (justfile:3310-3315). The current `not-a-gate laya-weights-bucket` row (last line of `scripts/laya_gates.tsv`) must go, as the plan says.
- The gates fake aws exists with `FAKE_AWS_MODE`/`FAKE_AWS_LOG`, account `123456789012`, unknown calls exit 97 (justfile:3355-3392); the `bucket-absent` branch the plan adds is the minimal change.
- Mutation M3 (`!= 1` → `> 1`) is detected even if the renderer dies with a KeyError on the missing `[aws] region` rather than the named refusal, because R4 requires the exact message.

**Concerns.**
- MEDIUM: `_laya-region-selftest` R3 writes the augury-dev config path once 13-04 moves it under `models/decide/augury-dev/`, and the plan only backs up and restores the default path. With `LAYA_SELFTEST_SERVER=aprender-mcp-decide-motivation` (13-04 Task 2, 13-06 Task 2, 13-07 step 5) R3 overwrites the real augury config with a `dry-run-bucket` render. 13-07 then asserts `augury_config_unchanged true` after the selftest. See 13-07 for the consequence; the fix belongs here or in 13-04 item 7.
- LOW: Task 2's full `laya-deploy-selftest` run (default SRV antisemite) rewrites `eligibility-aprender-mcp-decide-antisemite.log` and `eligibility-upload-…antisemite.log` via cases 4/5 (justfile:2345-2347; written at 1998 and 2799) without restoring them. Those default-path files currently hold the augury antisemite record. Pre-existing behaviour, fixed by 13-04 item 8, but 13-01 runs first.
- LOW: the pre-phase equivalence render targets `models/decide/deploy-config-aprender-mcp-decide-antisemite.toml`, the only local record of the augury antisemite deploy. The verify backs it up with `cp -p` and restores on EXIT, which is adequate, but a SIGKILL mid-verify leaves the dry-run config in place. No manifest protects that file.
- LOW: the `laya-weights-bucket` recipe runs `head-bucket` without `--region` (justfile:2751); the BUCKET table asserts only the create-bucket argv. Acceptable for D-20, just note the profile region is what resolves the rest.

**Suggestions.**
- Make `_laya-region-selftest` back up and restore (cp -p) both `_laya-state-dir ze-kasher-dev` and `_laya-state-dir augury-dev` config paths for SRV, or have 13-07 run the selftest before rendering.
- Add the eligibility-log restore to this plan's selftest change rather than waiting for 13-04.

**Risk: LOW.** Offline, well-bounded, and the mutation table covers each new branch.

---

## 13-02

**Summary.** Declares both partner-facing formats as one `kind: schema` contract and keeps the derived corpus files current. `ContractKind::Schema` exists (`crates/aprender-contracts/src/schema/kind.rs:39,66`) and is "exempt from provability, validated for metadata". The README gate tolerates lag (`readme_contract.rs:199-203`, `claimed <= contract_count`). The ratchet script does not measure `formal_prose` itself (`check_ont_ratchet.sh:89-98`, owned by other gates), and `contracts_without_valid_under` is counted at top level (baseline 386), so a new contract carrying `valid_under` cannot raise it. The plan's hygiene steps match what 12-02 did.

**Strengths.**
- The served labels and pins are checked against the real `data/decide/motivation-v1/task.json` (present locally) rather than restated.
- The mapping section quotes the guide sentences instead of "10.3", which RESEARCH showed is the examples section.
- 17 closed refusal names give 13-03 a concrete mirror target.

**Concerns.**
- LOW: Task 2 lints the single file. RESEARCH notes ontology gates arm only inside `contracts/`; the ONT-7 test runs `pv lint contracts/ --gate valid-under` on the whole corpus, so that path is covered, but a plain `"$PV" lint contracts/` would be the cheaper belt-and-braces.
- LOW: `valid_under` on a non-kernel kind is accepted by the ONT-7 lint (the `valid-under-nonkernel-bad` fixture rejects only a bad world), but no fixture shows a *good* non-kernel `valid_under` passing. Expect PASS; if it declines, drop `valid_under` rather than touch the baseline.
- LOW: wave-1 parallelism. 13-01 and 13-02 both commit from the same checkout unless each plan runs in its own worktree; interleaved `git add`/`commit` on one index can collide. Harness concern, not a plan defect.

**Suggestions.**
- Lint the directory in Task 2 and record the armed-gate list the lint prints.
- Pin `references` to existing files only (the plan already says so); `pv validate` on schema kinds checks them.

**Risk: LOW.**

---

## 13-03

**Summary.** The ingest design matches the code. `build()` groups by `normalized_sha256`, draws eval with `RandomState(SPLIT_SEED)` over `kept` (`prepare_motivation.py:166-190`), so any enrichment row entering `kept` would move eval; the "merge after the unchanged build with eval/mixed hashes" design is the only correct one. `load_rows` accepts only `text` and `label` (`data.py:ROW_KEYS`, `172-200`), which forces the manifest file. `write_once` compares only the files in the dict and refuses foreign non-dotfiles (`prepare_stance.py:161-178`), so including the manifest in `files` is required and sufficient. `_capture` and the sentinel control already exist (`prepare_motivation.py:~380, ~470`). The byte-identity baseline is reproducible locally (`data/decide/motivation-v1/` holds task/train/eval).

**Strengths.**
- `check_v1_identity` turns D-10 into an in-run check; the N1 mutation reaches it because `check_pin` only compares sha and counts, not the seed.
- The order eval-overlap → partner-mixed → conflict groups → collapse is correct and the non-resurrection case (mixed groups stay dropped) is a real gap that a naive union would open.
- The refusal style (row, field, count only) and the `--enrichment-sha256` pin follow the existing script exactly.
- The coverage assertion that every one of the 17 refusal names is produced by a case is a good guard against dead rule names.

**Concerns.**
- MEDIUM: D-12's "drop the partner row too" means train.jsonl is no longer a prefix-extension of v1 whenever a conflict exists. Task 1's verify asserts the prefix only for the conflict-free fixture, which is fine, but the manifest should carry `base_train_sha256` and the final sha so a retrain can tell "appended" from "edited". The plan does include both; keep them.
- LOW: Task 1's verify writes the merged `train.jsonl` (real partner post text) into a `mktemp -d` outside the repo, then deletes it. RESEARCH did the same. It is a transient copy on the same machine, within the spirit of "never into this repo", but say so in the SUMMARY.
- LOW: the `enrichment-in-repo` check resolves against `contract.REPO`; a symlinked path from outside into the repo would pass `resolve()`. Use `Path.resolve()` on both sides (the plan says "resolved path", which does this).
- LOW: `calibration_split` is re-asserted after merge; with a conflict-group drop of a thin class it could fail with `train-class-too-small`. That is a correct refusal, just make sure it is reported under an `enrichment-*` name or passed through unchanged, not swallowed.

**Suggestions.**
- Add a selftest case where the conflict drop removes the partner row and assert the manifest's `dropped_conflict_rows_partner` and that `train_sha256 != base_train_sha256` while the row count equals base − 1 + added.
- State the transient-copy fact in 13-INGEST-EVIDENCE.json.

**Risk: LOW–MEDIUM.** The logic is right; the medium is only that a lot of new branches land in one script, mitigated by the mutation table.

---

## 13-04

**Summary.** Makes the (target, account, region) triple the key. The hazard is real: every per-server path is id-keyed (`CFG`/`LOG`/`SNAP` at justfile:1957, 2041-2043 with `rm -rf "$SNAP"`; `STATE` at 3021; upload log at 2799), the repo-root `.pmcp/active-target` says `dev` and is gitignored (`.gitignore:87`), and `~/.pmcp/config.toml` defines both `dev` (ze-kasher-dev, us-east-1) and `augury-dev` (augury-dev, eu-west-1), with `staging-test` also on profile ze-kasher-dev, so the committed table is the right shape. The seven wired recipes are exactly the laya recipes with `aws`/`cargo pmcp deploy` in their bodies (my grep: laya-deploy, laya-deploy-config, laya-deploy-verify, laya-grant, laya-teardown, laya-upload, laya-weights-bucket; the selftests are excluded by suffix).

**Strengths.**
- The precedent is observable: `models/decide/deploy-aprender-mcp-decide-antisemite.state/deployment.toml` already holds the augury endpoint.
- The per-profile state dir keeps default paths byte-identical, so Phase 12 recipes and gate rows are untouched.
- The WIRING guard over `just --dump --dump-format json` reuses the drift check's own enumeration (justfile:3245-3255).
- Item 8 (restore the eligibility logs) closes the one writer of the ze-kasher-dev record that the existing selftest does not restore (justfile:2275-2284 restores only CFG and PROOF).

**Concerns.**
- MEDIUM: `_laya-target-check` inside `laya-teardown` adds a refusal path to the containment recipe that `laya-deploy`'s `contain()` (justfile:2044-2048) relies on. If the check refuses (stale env, config parse error), a bad deploy stays uncontained with only the printed fallback. The exports happen in `laya-deploy` before `contain()`, so the nested check should agree, but there is no case proving "teardown is still reachable from contain()". Add a WIRING or K case for it, or make the teardown check warn-and-continue rather than refuse.
- MEDIUM: Task 2 says the live endpoint check in `laya-deploy-verify` runs "before any AWS call or probe build", but the recipe builds the probe and sizes the shapes before the DRY_RUN branch and before STATE is read (justfile:3011-3021). The executor must reorder; otherwise the check lands after the build (harmless, but the truth line would be false).
- MEDIUM: see 13-01 — R3 of the region selftest and ISOLATION 1 both write the augury-dev config path for SRV; item 6 restores it for `_laya-target-selftest`, item 7 does not say the region selftest does. Make both restore it.
- LOW: `AWS_REGION` is exported by the recipes; AWS CLI precedence is `--region` > env > profile, so this is consistent today, but a future recipe passing `--region` explicitly would bypass the agreement check. Not a defect now.
- LOW: the K-mutations don't include "laya-deploy stops exporting PMCP_TARGET" (the marker `dev` would then win). K1 removes the check call; consider a K6 that removes the export.

**Suggestions.**
- Add `TEARDOWN REACHABLE` to `_laya-target-selftest`: with exports set as laya-deploy sets them, a DRY_RUN teardown against the fake aws must not be refused by the target check.
- Add K6 (drop `export PMCP_TARGET`) and assert the WIRING guard also requires the export line.

**Risk: MEDIUM.** The isolation mechanism is right; the risk is the new refusal paths in safety-critical recipes (teardown, deploy) that the mutation table does not yet exercise from the contain side.

---

## 13-05

**Summary.** A seeded, declared selection rule plus a note. The arithmetic checks out: 300 → 75/120/105 with 45/30/30; 100 → 25/40/35 with 15/10/10 (integer formulas as stated). Training prevalence (conspiratorial 41 of 295) is the right reason to over-sample. The output carries `post_id`, `selection_slice`, `rank_key` only, which satisfies D-03 structurally.

**Strengths.**
- Determinism under shuffled input and seed sensitivity are tested, and the band boundaries are tested at the exact edges.
- The record validator is bound to the 13-02 contract by a mirror case.
- `--exclude-data` reuses `data.normalized_sha256`, the one normalizer.

**Concerns.**
- LOW: the default `--exclude-data data/decide/motivation-v1` only exists on the aprender operator's machine; the note's "how to run" should say the partner runs it with `--no-exclude` or the aprender side runs it on their records in place. The note sections cover this implicitly; make it explicit.
- LOW: the band is OR of margin < 0.20 and p1 in [0.45, 0.75]; on a three-class softmax with T≈4.65 (Phase 12 numbers), most posts may fall in the band, making "uncertainty" nearly the whole pool. The shortfall reporting handles the opposite case; consider reporting the band's share of the pool in the header so the partner can see when it is not selective.
- LOW: `select-out-in-repo` refuses output under the repo; the verify writes to `mktemp -d`, fine.

**Suggestions.**
- Add `band_share_of_pool` to the selection header counts.
- Add a selftest case for a pool where every post is band-eligible, asserting control is still drawn first and the quota slice is still filled from the remainder.

**Risk: LOW.**

---

## 13-06

**Summary.** Correct reading of the probe: `TOKEN_ENV = "APRENDER_DECIDE_PROBE_TOKEN"` (`examples/probe.rs:58`), env fallback at `:182`, `--bearer` on argv at `:128`, header `:43` "never printed", and `bearer_auth` on every call (`src/probe.rs:193-194`), so a bearer rides `initialize` too and the stub's first `/mcp` request will carry it. CONTEXT's "no bearer path today" is half wrong, as RESEARCH says; the plan correctly scopes the new work to mint-from-file, re-exec and the refusal probe. The refusal design (assert 401 on tools/list then tools/call, record initialize) matches P6 and the partner runbook ("`initialize` is open at the front door, but `tools/list` and `classify` answer HTTP 401").

**Strengths.**
- `os.execvpe` with the token only in the child env, `set +x`, stderr limited to host/status/expires_in/method, and a selftest asserting absence, with A1 proving the absence check fires.
- The loopback stub proves the bearer reaches the wire without a network, and the `--stub-mismatch` run proves the CHAIN case can fail.
- The re-exec guard (`APRENDER_DECIDE_PROBE_TOKEN` set without `LAYA_M2M_MINTED=1` is refused) closes Pitfall 4 for both deploy and verify.
- Item 4 restores the plan files and eligibility logs the RE-EXEC cases write; those are three of the twelve 13-11 files.

**Concerns.**
- MEDIUM: `--min-expires` floors of 1800/2400 s assume the owner's M2M client issues ≥40-minute tokens. Cognito allows app-client access-token validity down to 5 minutes; if the owner picks a short validity, every verify and deploy is refused with no override. Make the floor an env-overridable default and name it in the refusal.
- LOW: the helper's host check requires `token_url` host == endpoint host unless `LAYA_M2M_TOKEN_HOST` matches `…\.(pmcp\.run|amazoncognito\.com)$`. The partner runbook says the Cognito hosted-domain URL is the fallback; the regex admits it. Fine, but `counter-coach.eu-west.augury.pmcp.run` (the shared-pool client, A1) would also be admitted as a token host for the motivation endpoint. That is probably desired (shared pool), so document it rather than leave it implicit.
- LOW: `laya-probe-auth` refuses a second `--url` and the argv bearer flag by scanning `"$@"`; a value like `--bearer=...` (if the probe ever accepted it) would slip past. The probe only accepts `--bearer <value>` today (`:128`), so this is theoretical.
- LOW: the token is visible to same-user processes via the child's environment. Acceptable on a single-user workstation; note it.

**Suggestions.**
- `LAYA_M2M_MIN_EXPIRES` override with the plan's defaults.
- Add an A6 mutation: the helper passes the token on argv instead of env; the CHAIN case must still pass but a `ps`-style check or the stub's `authorization_bearer_matches` plus a log scan for the token should go RED. At minimum assert the exec'd argv does not contain the token.

**Risk: LOW–MEDIUM.** Mechanism is proven offline; the medium is operational (token validity assumptions).

---

## 13-07

**Summary.** The pre-deploy half: baseline re-check, D-18 probe, bucket check, config render, offline selftest, upload, owner login. Reads A and R from the record (12-DEPLOY-EVIDENCE.json: `models/decide/laya-motivation-v1.apr`, sha `d202f495…`), which matches the file on disk. `laya-upload` runs `laya-verify` before any AWS call (justfile:2795-2801) and is idempotent by head-object size (2820-2827), as the plan states. The owner-run branch mirrors 12-12 and Phase 10's classifier refusal.

**Strengths.**
- The D-18 gate is three read-only checks and a STOP; nothing is faked.
- `laya-deploy`'s (6a) bucket check (justfile:2024-2032) means a wrong-account config cannot reach `cargo pmcp deploy`; the plan leans on it correctly.
- The evidence regex allows only the two known account ids and rejects JWT shapes.

**Concerns.**
- HIGH (plan-internal ordering): step 4 renders the augury config, step 5 runs `LAYA_SELFTEST_SERVER=aprender-mcp-decide-motivation … laya-deploy-selftest`, and the plan asserts the augury config's sha256 is unchanged. After 13-04, `_laya-region-selftest` R3 (called by the deploy selftest) renders `models/decide/augury-dev/deploy-config-aprender-mcp-decide-motivation.toml` under `DRY_RUN=1` with bucket `dry-run-bucket`; unless 13-01/13-04 restore that path, step 5 overwrites the real config and the verify fails (`augury_config_unchanged`). If the executor "fixes" it by re-rendering after the selftest, the recorded order is wrong; if not noticed, `laya-deploy` in 13-08 refuses at (6a) (fail-closed, so not dangerous). Fix: run step 5 before step 4, and have the selftests restore the augury path.
- MEDIUM: the full `laya-deploy-selftest` now also runs `_laya-target-selftest` and `_laya-auth-selftest` (which packs the fixture and builds the probe) plus the bootstrap cross-build; on the live-deploy day this costs minutes before the login checkpoint. Acceptable, but budget it.
- LOW: step 2 P10 probes the ze-kasher-dev server with `--apr "$A"` unauthenticated; correct (auth off there).
- LOW: the Lambda readback of the ze-kasher-dev function needs `--region us-east-1`; the plan passes it. Good. The augury antisemite readback is recorded "observed, not claimed", consistent with CONTEXT.

**Suggestions.**
- Swap steps 4 and 5 and keep the "config unchanged after selftest" assertion as a check that the selftests restore the augury path.
- Record the three sub-selftest OK lines from the deploy selftest log in `selftest{}` so 13-10 can cite them.

**Risk: MEDIUM.** Only because of the step-4/5 ordering interaction; everything else is read-only or idempotent.

---

## 13-08

**Summary.** The publish, OAuth enablement and M2M proof. `laya-deploy` passes `--no-oauth` when the config says auth off (justfile:2056-2063), the 404 retry is bounded (`_laya-edge-health-get`, justfile:2171-2220, default 40 × 15 s under a 900 s cap), the 503 settle loop is 8 tries (justfile:2106-2118), and containment calls `laya-teardown` (2044-2048). The plan's outcomes table matches those paths. `models/decide/augury-dev/m2m-file-path` is covered by `.gitignore:62` (`/models/`).

**Strengths.**
- `publish_done_utc`, owner reply time and first 401 give an honest upper bound on the open window; the refusal check never sends tools/call while tools/list is open.
- The negative control with the antisemitism artifact and the ARN region/account parse make the triple observable, not inferred.
- The owner's reply is stored with the path redacted and the credential-shape regex covers `client_id`, `client_secret`, JWTs and absolute paths.

**Concerns.**
- MEDIUM: during the open window, `_laya-auth-refusal-check … 600` polls unauthenticated `tools/list` every 15 s. On an auth-off server each poll is a Lambda invocation; cheap per call, but it can keep an instance warm and bills the augury account for up to 10 minutes. Consider `LAYA_AUTH_POLL_SLEEP=30` for the live run.
- MEDIUM: RESEARCH A3 (a redeploy with `--no-oauth` may reset OAuth) is only "observe". The F-10-12-2 recovery in Task 1 is safe because OAuth is still off then, but any later redeploy (13-09 `reverify-once` does not redeploy; fine) must re-run the refusal check. The plan says so in context; put it in the hand-off's operational notes too (13-10 has it).
- LOW: Task 3's refusal path "the path resolves under the repository root" should also refuse a path under `models/` of another checkout of this repo, which it does not need to; fine.
- LOW: the `cargo pmcp deploy oauth enable --copy-from` CLI option is offered to the owner as unverified (A2). Keep it labelled as such in the checkpoint text (it is).

**Suggestions.**
- Raise the poll interval for the live window, and record `polls` in the evidence.
- After Task 3, run one unauthenticated `tools/list` again (it is in 13-09 Task 3 already) and additionally after any future redeploy.

**Risk: MEDIUM.** Inherent to a live publish on a new tenant; the plan's stops and records are appropriate.

---

## 13-09

**Summary.** Authenticated timings and acceptance replay, plus after-checks. `laya-deploy-verify` bumps `DECIDE_COLD_BUMP` per sample (justfile:3040-3047), reads CloudWatch for `performed_load=true probe_id=…` (3056-3063), never contains, and writes `deploy-evidence-<server>.json` (3085-3110); the 12-13 normalization the plan copies matches that output shape (12-DEPLOY-EVIDENCE.json `live{}`). P=97 and door 8 come from 12-TASK-EVIDENCE.json and 12-DOOR-EVIDENCE.json as the plan reads them. The seven requester tests mirror 12-ACCEPTANCE-REPLAY.json.

**Strengths.**
- Every probe in verify inherits the minted token via the re-exec, so cold samples are authenticated without touching Rust.
- The step-down rule (only on `classify_max_total_tokens`) and the B+1 count refusal are carried verbatim from 12-13, which already observed 8 texts refused at 850 tokens.
- The after-checks compare the ze-kasher-dev Lambda keys with the 13-07 baseline and re-verify the 13-11 manifest.

**Concerns.**
- LOW: `_laya-auth-refusal-check "$URL" 0` sends a `tools/call` with one synthetic text only after a 401, so no compute; good. But the tools/list body is sent without `initialize`; on this platform that is exactly the P6 shape, so fine.
- LOW: the token record keeps `expires_in`; combined with `date`, that reveals nothing sensitive. OK.
- LOW: `reverify-once` appends `live.verify_attempts[1]`, but Task 1's normalization does not declare `verify_attempts`; 12-13 did. Add it so the verify's `rc == 0` branch is unambiguous after a re-run.

**Suggestions.**
- Declare `verify_attempts[]` in live{} as 12-13 did.
- Record the poll count and wall time of the closing refusal check.

**Risk: LOW–MEDIUM.** Live but read-mostly; the only writes are the deliberate cold bumps on the augury function.

---

## 13-10

**Summary.** Hand-off from records and a closure that re-establishes every claim itself. The out-of-scope surface list is correct: no plan edits `crates/aprender-mcp-decide-lambda` (the template at `.pmcp/deploy.toml.template` is only copied by the selftests), the thin crate is only `touch`ed, and `crates/.pmcp/deploy.toml` is restored byte-identical by `_laya-crates-root-swap` (justfile:1836-1899). The census/graph regeneration follows 12-02.

**Strengths.**
- The closure verify re-runs the fast suites and requires the two long logs to be newer than the last code commit, so a stale record cannot pass.
- The hand-off check compares every quoted value with its record and refuses the M2M path, JWTs and unknown account ids.
- The two-deployment triple table directly answers D-06's "cannot confuse".

**Concerns.**
- MEDIUM: the full `laya-gates-selftest` sweep now runs `laya-deploy-selftest` twice (armed and disarmed rows) and each run includes the bootstrap cross-build plus the region, target and auth selftests (the last builds the probe). Expect the sweep to grow by tens of minutes. Not wrong, but budget it and run it under the real-weights lock as the plan says.
- LOW: the hand-off's "Numbers and their records" regex requires record names like `13-X-EVIDENCE.json`; `13-SELECTION-NOTE.md` matches and exists, fine. `10-LEARNINGS.md` is skipped by the phase map; fine.
- LOW: the hand-off should not quote the partner HEAD `0c46b68` from RESEARCH (now `d822f119…`); quote the value 13-09 recorded.
- LOW: the sweep's 12-digit rule over SUMMARY files will trip on any epoch-ish number an executor writes; 13-11 already forbids epoch integers in its evidence, extend that rule to the SUMMARYs in the output instruction.

**Suggestions.**
- Add an estimated runtime line for the full sweep in Task 2 and allow the executor to run it in the background with the log path pinned.
- In section 9 of the hand-off, state explicitly that OAuth state must be re-checked after any redeploy (RESEARCH A3).

**Risk: LOW.**

---

## 13-11

**Summary.** A wave-1 baseline of the ze-kasher-dev local record with a sha256 manifest, a pre-phase mtime anchor and a committed pin. I verified the premise: all twelve files exist and every mtime is on 2026-10-26 (earliest `resolver-proof` 19:45Z, latest `deploy-evidence` 22:21Z), well before the base commit `61586f888` (committer time 1793126461 = 2026-10-27T18:41:01Z), and `deploy-aprender-mcp-decide-motivation.state/deployment.toml` names the us-east endpoint. Neither `models/decide/ze-kasher-dev-archive-20261027` nor `models/decide/augury-dev` exists yet. The restore paths the later plans rely on all use `cp -p` (justfile:2278-2282 for the deploy selftest, 3228-3238 for the gates selftest), so content and mtime survive a normal run; a killed run is exactly what the manifest catches.

**Strengths.**
- Running first in wave 1 and proving the anchor by mtime closes the "certify what a killed selftest left behind" hole RESEARCH flagged.
- No `just` is run, so 13-01's in-place justfile mutations cannot interfere.
- The coupling note is accurate: 13-01's default SRV is antisemite, its filtered sweep excludes `deploy-verify`, and the gates selftest restores `plan-*.json` with `cp -p` (justfile:3231-3236).

**Concerns.**
- LOW: idempotency step 1 requires every original's mtime still below BASE_TS on a re-run; a later wave's selftest restoring with plain `cp` (not `-p`) would change mtimes without changing bytes and make a re-run of 13-11 STOP. Only 13-11 checks mtime, and it runs once in wave 1, so this is theoretical; keep step 1's mtime requirement only for the first run, or compare by sha on re-runs.
- LOW: `git cat-file -e 61586f888…^{commit}` in the executor's zsh shell: braces without a comma are literal in zsh, so it works, but quote it anyway.
- LOW: the archive dir name embeds the date; if the phase slips past 2026-10-27 the name is still fine, but the plans hard-code `ze-kasher-dev-archive-20261027` in six places. Keep the literal.

**Suggestions.**
- On re-run, verify by sha and only warn on mtime drift.

**Risk: LOW.**

---

## Cross-plan assessment

**Dependency order.** The wave layout is right, including 13-11 in wave 1. The one ordering defect is inside 13-07 (render before the selftest that re-renders the same path), caused by 13-01/13-04 not restoring the augury-dev config path in `_laya-region-selftest`. Fix at either end; both is best. 13-03/13-04 coupling (in-place mutation of `prepare_motivation.py` while 13-04 sweeps) is correctly handled by the filtered row list, and is moot under per-plan worktrees.

**Does the phase achieve its goal?** Yes, with the fixes above. The deploy path is assembly of proven recipes plus three new offline-proven guards; the ingest is a bounded extension whose byte-identity is checked in every run; the formats are declared before any file exists; the hand-off is record-bound. The owner checkpoints (login, OAuth, M2M) are the only non-autonomous steps and are where the plan stops rather than fakes.

**Security.** The M2M secret never touches argv, logs or committed files by construction and the absence is tested; the token lives only in a child environment. Partner post text is read in place; the only transient copy is 13-03's merged `train.jsonl` in a temp dir, which should be stated. The second AWS account is addressed only through the target check, `env -u` prefixes and the (6a) bucket check, and every write is content-addressed or a single `cargo pmcp deploy`.

**Scope.** No creep into the verifier, served tool, door or register (13-10 proves it). The main over-engineering risk is runtime: the deploy selftest now nests three more selftests and a probe build, and the full gates sweep runs it twice.

**Overall risk: MEDIUM**, driven by the live tenant steps (13-07..13-09) and the two correctable plan defects: the augury-config overwrite in 13-07 step 5, and the new refusal path in `laya-teardown` that the containment safety net depends on.

---

## Consensus Summary

Two source-grounded reviewers (Codex gpt-6.1-sol at high reasoning; Claude Fable 5.1 as a reviewer instance) each covered all 11 plans with `file:line` evidence. Codex rates the phase **HIGH** risk and recommends revising before execution. Fable rates it **MEDIUM** and says the goal is achieved once its two plan defects are fixed. The orchestrator checked four findings against source before recording them; each is marked **[verified]** below.

### Agreed Strengths
- **Split preservation (13-03).** The merge runs after the unchanged v1 `build()`, so eval membership, which depends on the seeded permutation over the kept groups (`prepare_motivation.py:166-190`), cannot move. Provenance lives outside the closed `text`/`label` row schema (`data.py` ROW_KEYS).
- **Bearer path (13-06).** The plan reuses the existing one (`examples/probe.rs:182`, `src/probe.rs:193`) and needs no Rust change. The refusal check targets `tools/list` and `tools/call`, not `initialize`, which matches the partner's smoke logic.
- **State isolation (13-04).** The per-profile state dir addresses a real destructive path: `rm -rf "$SNAP"` (justfile:2042) and the id-keyed STATE (justfile:3021).
- **Hash baseline (13-11).** Taking a baseline and pinning it before any writer runs is the right move. Both reviewers checked that the protected files correspond to real writers.
- **Formats (13-02).** `kind: schema` is a supported home (`kind.rs:39-40`), and the README count gate tolerates lag (`readme_contract.rs:188-203`).
- **Owner checkpoints.** The owner-only checkpoints (login, OAuth enablement, M2M client) and the D-18 stop-don't-fake gate are appropriate.

### Agreed Concerns
1. **The selftests' backup/restore breaks or misses real consumers (13-01, 13-04, 13-07).**
   - **Codex, HIGH [verified]:** `armed_deploy_selftest` copies `eligibility-$SRV.log` *after* the child returns (justfile:3442), and `row_verify` requires that log to show `deploy_eligible true` for the deployed sha (justfile:3608). Once 13-04 item 8 restores the log on EXIT, the gate reads the historical log instead. That log already says eligible, so the check goes vacuous rather than failing.
   - **Fable, HIGH:** `_laya-region-selftest` R3 renders the augury-dev config path, which 13-01/13-04 do not restore. 13-07 renders the real augury config in step 4, then runs the selftest in step 5 and asserts the config is unchanged, so step 5 overwrites what step 4 wrote.
   - **Common root:** the restore design does not enumerate every path each selftest writes, nor every reader of those paths.
2. **The open-auth window and the auth hand-off are weaker than claimed (13-06, 13-08).**
   - **Codex, HIGH [verified]:** the partner's client mints with form-body `client_id`/`client_secret` only (`motivation-experiment/crates/core/src/auth.rs:52-56`), so a Basic-only mint cannot count as partner-ready.
   - **Codex, HIGH:** `urllib` follows POST redirects and carries `Authorization` across hosts, and the helper has no redirect policy.
   - **Codex, HIGH:** `publish_done_utc` is taken after the whole deploy recipe returns, so first-401 minus that time is not an upper bound on the exposure. There is also no bound if the owner never replies.
   - **Fable, MEDIUM:** the `--min-expires` floors of 1800/2400 s assume a long Cognito token validity and have no override.
3. **Verifies accept weaker outcomes than their truths state (13-05, 13-09, 13-10).**
   - **Codex, HIGH [verified]:** 13-09 accepts a `classify_max_total_tokens` refusal for the batch of four (13-09-PLAN.md:262) while claiming a successful four-text batch.
   - **Codex:** `reverify-once` has no second-failure branch, and `unchanged: true` is trusted instead of recomputed.
   - **Fable:** `verify_attempts[]` is not declared in `live{}`.
   - **Codex, 13-05:** the shuffled-input byte-identity check contradicts the input-sha header, and the note-vs-code number check only searches for numeric strings.
   - **Codex, 13-03:** "manifests differ only in annotation counts" contradicts per-source byte hashes.
4. **Runtime cost.** Both reviewers note that the full gates sweep now runs the deploy selftest twice, nesting the region, target and auth selftests, a probe build and the bootstrap cross-build. Budget for it, and keep it off the critical path before the owner login.

### Divergent Views
- **Target-guard completeness (13-04).**
  - Codex raises two HIGHs:
    - `PMCP_API_URL` is not guarded or unset, and cargo-pmcp's resolver prefers the env value over the target's (`resolver.rs:250`) **[verified]**.
    - The target table carries no expected account ids, so a correctly named profile pointing at another account passes offline.
  - Fable did not raise these. It focused instead on a new refusal path inside `laya-teardown`, which containment depends on (justfile:2044-2048), and asked for a "teardown still reachable from contain()" case plus a K6 mutation that drops `export PMCP_TARGET`. These findings are complementary, not contradictory.
- **13-11 mtime anchor.** Codex says an old mtime does not prove the bytes predate the phase, since `cp -p` preserves timestamps. Treat it as supporting evidence and resolve resume-time re-pins against the committed pin. Fable accepts the anchor and asks only that re-runs compare by sha.
- **Overall risk.** Codex says HIGH, concentrated in wrong-tenant prevention and credential compatibility. Fable says MEDIUM, concentrated in the live steps and two correctable defects.
- **CI wiring (13-10).** Codex wants the synthetic ingest, selection and credential selftests wired into CI fragments. The plans deliberately keep laya selftests local-only, like every earlier one. This needs an owner call.
- **Stale facts noted by Fable.** The partner HEAD is now `d822f119…`, not `0c46b68`, so the hand-off must not quote the old value. The pmcp.run session has been refreshed (it expires 2026-10-27T21:32:20Z).

### Recommended next step
`/gsd-plan-phase 13 --reviews`. Address agreed concerns 1–3 and the verified 13-04 target-guard gaps. Decide the CI-wiring divergence explicitly.
