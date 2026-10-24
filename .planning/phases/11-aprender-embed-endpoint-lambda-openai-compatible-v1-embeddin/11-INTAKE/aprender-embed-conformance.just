        cargo test -p mcp-rag-server-core --features aws,lambda --test localized_generator_live -- --ignored --nocapture 2>&1 | tee "$out"
    grep -q 'LIVE-PROOF generator ' "$out" || { echo "ERROR: no 'LIVE-PROOF generator ' marker -- the live proof did not run" >&2; exit 1; }

# Phase 205-06 (D-07) CONFORMANCE of a DEPLOYED aprender-embed Lambda function: the thirteen checks named
# in 205-APRENDER-EMBED-HANDOFF.md "Acceptance", run through the production lambda:// transport (the
# operator's current AWS session; the recipe never names a profile or account). Billed: a few dozen
# Lambda invocations. <base_url> must be the lambda:// form; <reference> is the delivered
# aprender-embed-reference-vectors.json. Ignores any configured AWS endpoint so the run cannot be
# redirected; fails unless `CONFORMANCE RESULT PASS 13/13` is printed. Not part of test-all.
#   just aprender-embed-conformance lambda://aprender-embed-endpoint-dev/v1/embeddings ./aprender-embed-reference-vectors.json
aprender-embed-conformance base_url reference:
    #!/usr/bin/env bash
    set -euo pipefail
    test -n "{{base_url}}" || { echo "ERROR: base_url is required (lambda://<function>/v1/embeddings)" >&2; exit 1; }
    test -f "{{reference}}" || { echo "ERROR: reference-vectors file not found: {{reference}}" >&2; exit 1; }
    # cargo runs the test binary from the crate directory, so hand it an absolute path.
    ref_abs="$(cd "$(dirname "{{reference}}")" && pwd)/$(basename "{{reference}}")"
    out=$(mktemp)
    trap 'rm -f "$out"' EXIT
    APRENDER_EMBED_BASE_URL="{{base_url}}" APRENDER_EMBED_REFERENCE="$ref_abs" AWS_IGNORE_CONFIGURED_ENDPOINT_URLS=true \
        cargo test -p snapshot-build --features aprender-lambda --test aprender_conformance -- --ignored --nocapture 2>&1 | tee "$out"
    grep -q 'CONFORMANCE RESULT PASS 13/13' "$out" || { echo "ERROR: no 'CONFORMANCE RESULT PASS 13/13' line -- the deployed function is not conformant (or the probe did not run)" >&2; exit 1; }

