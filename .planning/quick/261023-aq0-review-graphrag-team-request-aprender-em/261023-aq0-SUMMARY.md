---
status: complete
type: review (no code changed)
---
# Review: GraphRAG request — aprender-embed-endpoint Lambda

Verdict: **feasible, mostly assembly; scope as a new phase (11).**

## Findings from the tree (2026-10-23)
- Premise is partly stale: `/v1/embeddings` in aprender-serve is NOT a zeros stub any more
  (`realize_handlers_embed_completion.rs::embed_inputs`: hidden states -> mean pool -> L2).
  But it is the decoder/GGUF path: no 256-token cap, wrong encoder for MiniLM. Not reusable here.
- The right engine already exists: `aprender-core::setfit` is a native all-MiniLM-L6-v2 encoder
  (pinned HF revision in `setfit/import.rs:61`), `MAX_SEQUENCE_LENGTH = 256` incl. [CLS]/[SEP]
  (`setfit/tokenizer.rs:52`, truncation reported via `TruncationFact`), mean-pool + L2, and a
  no-panic `SetFitArtifact::embed` (artifact.rs:1875). Server-side truncation (item 1) is inherited.
- `aprender-mcp-setfit-lambda` is a loopback pmcp proxy, not an API-GW-v2 handler: reuse only the
  build/embedding/model-resolve pieces, write a direct `lambda_runtime` handler.
- Item 3 (no ingress, IAM-invoke only) conflicts with the pmcp.run/`cargo pmcp deploy` path, which
  creates ingress. Needs a CDK/`aws lambda` deploy under `deploy-extensions/` like the training stack.

## Open items / risks
1. Reference vectors need Python sentence-transformers at max_seq_length=256 (HF download).
2. `aprender-embed-conformance` and the 13 checks live in the pmcp.run repo: need the brief to run them.
3. Deploy needs owner-run AWS creds (as in phases 8/10); cold start (model load + 1 embed) unmeasured.
4. Premature to promise memory/provisioned concurrency: measure first (10 GB tier is available).
5. Hand-off statement needs `.apr` + safetensors + tokenizer.json + vocab.txt sha256s; check packer emits vocab.txt.
