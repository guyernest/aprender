# Requester answers — pmcp.run graph-rag → aprender Phase 11

Sent 2026-10-23. Source checkout: `github.com/guyernest/pmcp-run` (private), directory `built-in/graph-rag`, HEAD `21f1822a1` at send time. No account ids, ARNs or keys appear in this folder.

## Item 1 — the brief

`11-BRIEF.md` in this folder is the body of `205-APRENDER-EMBED-HANDOFF.md`, verbatim. It is the brief we intend. Its "Acceptance" section is the thirteen checks. One line in it is stale: the sentence that says `apr serve`'s `/v1/embeddings` is a zeros stub. Your team already corrected this; the conclusion (build a purpose-built server) is unchanged.

## Item 2 — the tool `aprender-embed-conformance`

- Repository: `github.com/guyernest/pmcp-run`, directory `built-in/graph-rag`.
- Recipe: `aprender-embed-conformance base_url reference` in `built-in/graph-rag/justfile` (copied here as `aprender-embed-conformance.just`).
- Check definitions: `built-in/graph-rag/crates/snapshot-build/tests/aprender_conformance.rs` (copied here verbatim). Last changed in commit `602efc9a6` (2026-10-22).
- It is an integration test of the `snapshot-build` crate: it imports `snapshot_build::providers::aprender` and `::lambda_invoke`, so it does not compile standalone. Run it from a checkout of `built-in/graph-rag`.
- Run command (needs an AWS session for the account that hosts the function; never names a profile; refuses to run if any `AWS_ENDPOINT_URL*` variable is set):

  ```bash
  cd built-in/graph-rag
  just aprender-embed-conformance lambda://aprender-embed-endpoint-<stage>/v1/embeddings ./aprender-embed-reference-vectors.json
  ```

  Underneath: `APRENDER_EMBED_BASE_URL=... APRENDER_EMBED_REFERENCE=<abs path> AWS_IGNORE_CONFIGURED_ENDPOINT_URLS=true cargo test -p snapshot-build --features aprender-lambda --test aprender_conformance -- --ignored --nocapture`.
- Output: one line per check, `CONFORMANCE <check> PASS|FAIL <detail>`, then the verdict line. The exact pass line is:

  ```
  CONFORMANCE RESULT PASS 13/13
  ```

  The recipe exits non-zero unless that exact line is printed.
- The thirteen check names, in output order: `shape`, `l2-normalized`, `non-degenerate`, `deterministic`, `batch-order-invariant`, `long-input`, `truncation-upper`, `truncation-lower`, `reference-vectors`, `semantic-sanity`, `model-echo`, `empty-string-400`, `wrong-dimensions-400`.
- The hermetic self-tests in the same file (no feature, no AWS) show which check each known-broken server shape fails; they run with a plain `cargo test -p snapshot-build --test aprender_conformance`.

## Item 3 — reference-vectors schema

`aprender-embed-reference-vectors.schema.md` and `aprender-embed-reference-vectors.sample.json` in this folder. Only `texts[].id` and `texts[].vector` (384 numbers) are required; `texts[].text` is optional but checked when present; the release-identity fields are kept but not read.

## Item 4 — the captured event

`apigw-v2-platform-captured.json` in this folder (your plan wants it at `deploy/embed-endpoint/events/apigw-v2-platform-captured.json`). It is byte-for-byte what `build_http_event` produces, with a two-input body and request id `graphrag-embed-0`; the only per-call variation is the `requestId` counter.

Who builds it: graph-rag's own tool — `build_http_event` in `built-in/graph-rag/crates/snapshot-build/src/providers/lambda_invoke.rs`. The same builder serves the snapshot build, the server's cold-start canary and the conformance probe. pmcp.run's durable agent has its own copy of the same shape for LLM endpoints; it is not on this path. Constants: `user-agent: pmcp-graphrag` (do not depend on it), `rawQueryString` always empty, `body` always a JSON string, `isBase64Encoded` always false on the way in. The server must still accept `isBase64Encoded: true` with a base64 body, per the brief.

Response decoding: graph-rag reads only `statusCode` (integer), `body` (string) and `isBase64Encoded` (defaults to false). A payload that is not that envelope, or an invoke with `X-Amz-Function-Error` set, is a provider error.

## Item 5 — function-name allowlist

Yes. graph-rag invokes only `lambda://<function-name>/v1/embeddings` where `<function-name>` starts with `aprender-embed-`. The prefix is fixed in `built-in/graph-rag/crates/snapshot-build/embedding-providers.json` (`providers.aprender.function_name_prefix`, copied here) and is deliberately not environment-overridable. The name is a bare function name, 1–64 characters of letters, digits, `-` and `_`; no ARN, qualifier, alias, query string or fragment; the path must be exactly `/v1/embeddings`. `aprender-embed-endpoint-<stage>` satisfies the rule. The `aprender-llm-` / `pmcp-llm-` prefixes belong to the durable agent's LLM `lambda://` allowlist, a different transport, and do not apply here.

## Item 6 — envelope policy

a. `usage.prompt_tokens`: unspecified. graph-rag never reads `usage`; any non-negative integers are accepted. If you want a rule, count positions consumed after truncation, special tokens included.
b. Empty string: refuse with 400 and the OpenAI error envelope `{"error":{"message","type","code"}}` (check `empty-string-400`). graph-rag never sends one; it zero-fills empty and whitespace-only inputs client-side. Empty array: never sent; a 400 or a 200 with an empty `data` list are both acceptable.
c. Integer-token inputs: out of scope. Never sent. A 400 is fine.
d. `dimensions`: absent or `384`. Any other value is a 400 (check `wrong-dimensions-400`). graph-rag never sends the field.
e. Unknown request fields: unspecified. graph-rag sends exactly `model` and `input`. Ignoring unknown fields is fine; refusing them is also fine provided those two are accepted.
f. 429: nothing is required to produce one. graph-rag maps a 429 envelope to a transient, retryable error, the same class as Lambda's own `TooManyRequestsException`. Emit it only if you add an in-function admission limit; otherwise never.
g. Batch size: at most 64 inputs per invoke and at most 5 MiB of serialized event; at most 8 invokes in flight per build (`DEFAULT_CONCURRENCY`). Typical input is one chunk of a few hundred to about 2,000 characters; the long tail reaches about 1,600 wordpieces (the brief's long-input case). The probe's own batches are small (one to a few dozen short texts).
h. Model id: exactly `all-MiniLM-L6-v2`, no aliases. The response `model` must echo that exact string (check `model-echo`).
i. Extra response headers: acceptable. graph-rag reads none of them.
j. Same account: yes, and the same region. Principal kind: an IAM role — the execution role of the graph-rag server Lambda and of the build Lambda, granted `lambda:InvokeFunction` on the one function ARN. Operator runs of the probe use the operator's own session credentials.
k. Stage names in the environment roster: `main` (the dev environment) and `prod` (every other environment). Region: the consuming environment's own; the fleet today spans us-east-1, us-west-2 and eu-west-1. First proof target: the **dev** environment — stage `main`, region **us-east-1**, so the function is `aprender-embed-endpoint-main`. First production target after that: the **augury-dev** environment — stage `prod`, region **eu-west-1**, function `aprender-embed-endpoint-prod`. Each is a separate AWS account; the operator's own session selects it (no profile or account id is ever written into code or config).

## Reply

intake-ready — material is in this folder (`11-INTAKE/`). Every item is answered; nothing is `unspecified` beyond 6a and 6e, which are deliberate.
