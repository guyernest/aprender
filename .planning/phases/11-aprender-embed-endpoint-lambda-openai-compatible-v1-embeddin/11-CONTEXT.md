# Phase 11 Context: aprender-embed-endpoint Lambda

Source: GraphRAG (pmcp.run graph-rag) request, 2026-10-23, reviewed in quick task 261023-aq0
(.planning/quick/261023-aq0-review-graphrag-team-request-aprender-em/261023-aq0-SUMMARY.md).

## Locked requirements (from the requester; verbatim intent)
- R1 Lambda `aprender-embed-endpoint-<stage>` serving POST /v1/embeddings: API Gateway v2 event in, OpenAI-shaped response out, for sentence-transformers/all-MiniLM-L6-v2 (384-d, mean pooling, L2).
- R2 Server-side truncation: full encoded sequence <= 256 tokens incl. [CLS]/[SEP]. A 1,600+ wordpiece chunk returns 200, never panics, never 5xx. Build-time and query-time vectors identical.
- R3 Release identity bundle per release: release tag, deployed CodeSha256, sha256 of .apr, safetensors, tokenizer.json, vocab.txt; HF revision; encoder config; `aprender-embed-reference-vectors.json` computed with sentence-transformers at max_seq_length=256.
- R4 No public ingress: no Function URL, API Gateway or load balancer; resource policy has no `*` principal; access by IAM lambda:InvokeFunction only.
- R5 Never an unhandled FunctionError: malformed input gets the OpenAI error envelope with 400, 413, 429 or 500.
- R6 Measure and REPORT (not promise): memory size and timeout (timeout >= 30 s), cold and warm latency for 1 input and for 64 realistic chunks, whether provisioned concurrency is needed. Cold start must answer a one-input canary within 30 s.
- R7 Acceptance: requester runs `just aprender-embed-conformance lambda://<function-name>/v1/embeddings <reference-vectors.json>` -> `CONFORMANCE RESULT PASS 13/13`. We run the equivalent on our own build first.
- Deliverables per environment: function name + region, release bundle, reference-vectors file + sha256, deployment-limit numbers.
- Out of scope: multilingual/larger encoders, Cohere, re-embedding tooling. Nothing changes in the pmcp.run repo. English-only, 384-d accepted.

## Findings that shape the plan (verified in tree 2026-10-23)
- Engine exists: `aprender-core::setfit` = native all-MiniLM-L6-v2, pinned HF revision (setfit/import.rs:61), MAX_SEQUENCE_LENGTH=256 (setfit/tokenizer.rs:52) with TruncationFact, no-panic `SetFitArtifact::embed` (artifact.rs:1875).
- aprender-serve `/v1/embeddings` is NOT a zeros stub any more but is the decoder/GGUF path: not reusable.
- `aprender-mcp-setfit-lambda` is a pmcp loopback proxy, not an API-GW-v2 handler. Write a direct lambda_runtime handler; reuse model resolution only.
- R4 conflicts with the `cargo pmcp deploy` path (creates ingress). Deploy via CDK/aws under `deploy-extensions/` (training stack style), IAM-invoke only.
- Phase 10 pattern to follow: thin per-server crate over shared library, fail-closed deploy recipes in justfile, owner-run live deploy, identity probe, evidence JSON, claims ledger.

<decisions>
## Owner decisions recorded by plan 11-02

- **D-01 (owner):** Option grow-new-crate approved, with the owner reply "grow-new-crate (Recommended)" recorded verbatim in 11-OWNER-DECISIONS.json (relayed by the coordinator, 2026-10-24 UTC). The deployment-unit register in crates/aprender-core/tests/monorepo_invariants.rs grows from 9 to 10 so the repository can hold a new publish = false crate, aprender-embed-endpoint, which is a library plus one binary of the same name serving POST /v1/embeddings from Lambda. The register category is read as publish = false deployment-unit binaries, because the endpoint is an IAM-invoke Lambda transport and not an MCP server; plan 11-06 amends the category comment and the assertion message in the same change. The baseline raise and the crate share one change, because this decision precedes it. The duplicate-bin allowlist is untouched. The crate and binary names are fixed as aprender-embed-endpoint.
- **D-02 (owner):** Option embed-texts-api approved, with the owner reply "embed-texts-api (Recommended)" recorded verbatim in 11-OWNER-DECISIONS.json (relayed by the coordinator, 2026-10-24 UTC). Plan 11-05 adds the public method SetFitBert::embed_texts(&self, texts: &[&str]) -> Result<EmbeddedTexts, SetFitError>, plus the public types EmbeddedTexts (with dim(), a row accessor, len() and facts()) and InputFacts (with truncated, original_len and consumed_positions). The encode runs under no_grad inside the method. The method does not call clear_graph, and encode_texts, embed and classify keep their behavior, and the constructor seal is untouched. The owner answered "Yes, panic-safe (Recommended)" (relayed by the coordinator, 2026-10-24 UTC), so the panic-safe no_grad scope is wanted: a crate-private RAII guard restores the previous grad flag during unwinding, recorded as panic_safe true with the behavior "grad flag restored on unwind". One item is still open and is not decided here: the name of the EmbeddedTexts row accessor, which the owner did not give. The planner proposes a name for owner confirmation before plan 11-05 writes code.
- **D-03 (owner):** The 64-input client-wall overrun measured in plan 11-04 is accepted as a known condition, not a blocker: client wall for the five 64-input samples was 31.7 to 38.9 s, while Lambda Duration fit 30 s (cold max 29.09 s, warm median 28.65 s). Owner reply, verbatim: "The slightly above cap is acceptable, as we have the mixed CPU, and we can expect more Graviton 3 in the future, and we know that in-account models are coming with latency tax, and the customers accept them." Reasons: mixed CPU (0xd0c and 0xd40 in the measurement), expected Graviton 3 growth, and customers accepting in-account latency. Recorded in 11-OWNER-DECISIONS.json under `admission_budget`. Not decided by this reply, and left open for plan 11-08's planner: which measure admission uses (Lambda Duration or client wall), later answered by D-05 (Lambda Duration). The cap question was answered by D-04 (32 inputs). The normalizer and the evidence file are unchanged.
- **D-04 (owner):** The endpoint's input cap changes from 64 to 32, for now. Owner reply, verbatim: "We can switch to 32-input for now." This answers the cap question D-03 left open. The admission measure (Lambda Duration or client wall) remains open for plan 11-08's planner. No 32-input sample was measured: the raw record has 1-input and 64-input samples only, so the 32 figure is an owner decision, not a measurement. It needs a measurement or derivation before plan 11-08 relies on it, and 32-input timings must not be inferred from the 64-input data. Recorded in 11-OWNER-DECISIONS.json under `input_cap`.
- **D-05 (owner):** Admission uses Lambda Duration, not client wall time. Owner reply, verbatim: "Let's run the 32-input measurements again, and use the lambda duration." This answers the admission-measure question that D-03 and D-04 left open. D-05 decides the measure, not the cap value: the cap stays at 32 under D-04 and is not yet measured. A 32-input measurement is approved (one redeploy, one run, one destroy, run by the owner) and has not been run. Recorded in 11-OWNER-DECISIONS.json under `admission_measure`.
</decisions>




## Open / blocked inputs
- The requester's brief with the thirteen conformance checks, and the `aprender-embed-conformance` tool (in the pmcp.run repo): not in hand. Plans must include a checkpoint to obtain them and must not invent the 13 checks.
- Live deploy needs owner AWS credentials (profile ze-kasher-dev); live steps are owner-run or checkpointed.
- Reference vectors need Python sentence-transformers + HF download (back-office, not served).
- Target date: not yet set; to be set after the cold-start measurement.

## Project rules to honor
justfile for scripts; strict clippy per touched crate; `pv validate` for any contract (new contract for the endpoint boundary); no unwrap; no new .sh without need; pin binaries; measure before reporting; report timings, do not gate on them unless the requester's numbers say so.
