# Phase 15 deferred items

## D-ITEM-15-04-A: corpus-wide contract hygiene was red at dd3c96937 and at HEAD (found during plan 15-04, Task 2) -- RESOLVED inline

Resolved in plan 15-04 with the user's authorisation (option a): commit 49a8df031 marks the two U+2717 obligations `prose: true`; commit 9b5e17101 gives the Phase 13 schema contract `motivation-enrichment-v1` one equation restating its own `no_text_rule` (it was the 445th data-integrity violation; the 444 ceiling and every baseline are untouched). HYGIENE and REGRESSION then passed on their own runs. The text below records the finding as first reported.

Out of Phase 15's scope (no Phase 15 commit touches the files below). Reproduced at the phase baseline dd3c96937 in a clean worktree; the
logs are local and gitignored under `models/laya-trainer/p15-gap/` (`head-*` and `baseline-*`).

| Stage (laya-gap-regression) | Failure | At dd3c96937 |
|---|---|---|
| HYGIENE `pv-lint` (`pv lint contracts`) | 2 x `PV-ONT-003`: `proof_obligations[2].formal` and `[3].formal` of `contracts/llm-endpoint-openai-conformance-v1.yaml` carry U+2717, which the sigma ontology does not declare; `armed meet: Fail (11 armed; not armed: reverse-coverage, valid-under)` | identical lines |
| HYGIENE `contracts-lib` (`cargo test -p aprender-contracts --lib`) | `lint::tests::lint_passes_on_real_contracts` fails, same cause | same test fails |
| HYGIENE `contracts-corpus` (`--test validate_contracts`) | `contract_data_integrity`: "Data integrity violations rose to 445 (ceiling 444) -- this list may only shrink" | the 447-line violation list is byte-identical (md5 cffd4b4bd6aaa07c5d9ebcffb7baba6e) |
| REGRESSION `contracts-cli` (`-p aprender-contracts-cli`, clean export) | `ont2b_sigma_gate::the_repo_corpus_passes_the_sigma_gate`, same PV-ONT-003 pair | not run (same root cause as pv-lint) |

Known earlier: 12-02-SUMMARY and 13-02-SUMMARY record the same two PV-ONT-003 errors as an observation ("a Phase 9 contract this plan does not touch").
The Phase 9 contract was authored 2026-10-15 (5218347bd6) and last changed 2026-10-16 (7e7e111f3f).

Candidate repairs as first listed (applied as described above):
1. `llm-endpoint-openai-conformance-v1.yaml`: mark `proof_obligations[2]` and `[3]` `prose: true`, or declare U+2717 in `contracts/ontology.yaml`.
2. Find the one contract that raised the data-integrity count from 444 to 445 and repair it (the ceiling may only shrink).
3. Re-run `LAYA_GAP_ONLY=HYGIENE` and `LAYA_GAP_ONLY=REGRESSION`.
