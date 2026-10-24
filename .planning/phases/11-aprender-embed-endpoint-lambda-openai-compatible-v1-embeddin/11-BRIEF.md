Source: pmcp.run built-in/graph-rag/.planning/phases/205-localized-embeddings-bedrock-titan-v2-and-in-account-aprende/205-APRENDER-EMBED-HANDOFF.md (requester checkout ~/Development/mcp/sdk/pmcp-run, body byte-identical to HEAD 369e8e79b; answers.md cites 21f1822a1 as HEAD at send time) Captured: 2026-10-23 sha256-of-body: 99fcb8562051cd1e95cd19ba1844e2c2b8ede347d356b461d4a70391fb851612
# Hand-off brief: `aprender-embed-lambda` (OpenAI `/v1/embeddings` for pmcp.run graph-rag)

**From:** pmcp.run graph-rag, Phase 205 (Localized embeddings), Plan 205-02
**To:** a planner in the aprender repository (`~/Development/machine-learning/aprender`), working in that repository's own GSD project
**Status:** contract v1, written 2026-10-22. Self-contained: you need no access to the pmcp.run repository. Every shape below is reproduced verbatim from the platform's existing `lambda://` transport (`lambda_transport.rs`, `amplify/functions/durable-agent-lambda/src/llm/`).

## Why

A graph-rag instance can choose, once at creation, to embed its corpus and its questions with an in-account MiniLM encoder instead of OpenAI (decisions D-01 and D-03: exactly one model and dimension per instance for the life of its vectors; the model is `all-MiniLM-L6-v2`, 384 dimensions). The consuming customer's text must never leave their AWS account and region, so the encoder has to run as a Lambda function in that account.

Today there is nothing to point at. `apr serve`'s `/v1/embeddings` is an Ollama-compatibility stub that ignores the request and returns 128 zeros (`crates/apr-cli/src/commands/serve/ollama.rs:631`), and `apr embed` panics (exit 101) above 512 wordpieces instead of truncating (spike 009 and 010; 2 of 189 real chunks were over the limit, up to 1,609 wordpieces). A real server must be built, following the `aprender-mcp-setfit-lambda` packaging pattern (custom runtime `bootstrap`, model embedded with `include_bytes!` or loaded from a path, warm model per container). Accepted limits, already decided by the owner: English-only, 384-d, and slower than OpenAI (about 15 texts/s/thread measured on an Apple M4 Pro with `apr embed`; Lambda speed is unmeasured and is one of the numbers this brief asks you to produce).

## Wire contract

**Transport.** A Lambda function in the CONSUMING environment's own AWS account and region, invoked synchronously with `lambda:InvokeFunction`. graph-rag addresses it as `lambda://<function-name>/v1/embeddings`.

**Addressing rule (stricter than the platform's durable-agent transport, which graph-rag does not copy as-is):**

- Bare function name only. No ARN, no qualifier, no alias, no cross-account form.
- No query string. The platform's transport accepts `?timeout=<seconds>`; graph-rag refuses any `?`.
- The function name must start with `aprender-embed-` (graph-rag's parse-time prefix rule). The platform's own allowlist is environment-overridable; graph-rag's is fixed in its contract file. Function names are 1 to 64 characters of letters, digits, `-` and `_`.

**Request event.** The caller sends an API Gateway HTTP API payload-format-2.0 event. This is exactly what `lambda_transport.rs` builds (`build_apigw_v2_event`); header names are lower-case:

```json
{
  "version": "2.0",
  "routeKey": "$default",
  "rawPath": "/v1/embeddings",
  "rawQueryString": "",
  "headers": {
    "content-type": "application/json",
    "accept": "application/json",
    "user-agent": "pmcp-durable-agent"
  },
  "requestContext": {
    "http": {
      "method": "POST",
      "path": "/v1/embeddings",
      "protocol": "HTTP/1.1",
      "sourceIp": "127.0.0.1",
      "userAgent": "pmcp-durable-agent"
    },
    "requestId": "<opaque request id>",
    "stage": "$default"
  },
  "body": "{\"model\":\"all-MiniLM-L6-v2\",\"input\":[\"first text\",\"second text\"]}",
  "isBase64Encoded": false
}
```

The `user-agent` value is whatever the caller sets; the server must not depend on it. `body` is a JSON string. The server must also accept `isBase64Encoded: true` with a base64 body.

**Response envelope.** The function returns the API Gateway v2 response shape. The caller reads `statusCode`, `body` and `isBase64Encoded` (the last defaults to false when absent; a payload that is not such an envelope is treated as a function error and the call fails):

```json
{
  "statusCode": 200,
  "headers": { "content-type": "application/json" },
  "body": "{\"object\":\"list\",\"model\":\"all-MiniLM-L6-v2\",\"data\":[{\"object\":\"embedding\",\"index\":0,\"embedding\":[0.0123, -0.0456, 0.0789]},{\"object\":\"embedding\",\"index\":1,\"embedding\":[0.0001, 0.0002, 0.0003]}],\"usage\":{\"prompt_tokens\":9,\"total_tokens\":9}}",
  "isBase64Encoded": false
}
```

(The example `embedding` arrays are shortened to three floats for readability; real ones hold exactly 384.)

**Request body.** `{"model": "all-MiniLM-L6-v2", "input": [strings]}`. A single string for `input` is also accepted and treated as a one-element list. `encoding_format` is absent or `"float"`. `dimensions` is absent or `384`. Any other value of either field is a 400. An unknown `model` is a 400.

**Response body (statusCode 200).**

```json
{
  "object": "list",
  "model": "all-MiniLM-L6-v2",
  "data": [
    { "object": "embedding", "index": 0, "embedding": [ "... 384 finite floats ..." ] }
  ],
  "usage": { "prompt_tokens": 0, "total_tokens": 0 }
}
```

`data` holds exactly one entry per input, with `index` values 0 to n-1, each exactly once. graph-rag rejects a short list, a duplicated index or an out-of-range index, so return them in input order with the right `index`. `model` echoes the served model id.

**Errors.** The OpenAI error envelope, as the `body` of a non-200 `statusCode`:

```json
{
  "statusCode": 400,
  "headers": { "content-type": "application/json" },
  "body": "{\"error\":{\"message\":\"input must not contain an empty string\",\"type\":\"invalid_request_error\",\"code\":\"empty_input\"}}",
  "isBase64Encoded": false
}
```

Status codes: 400 (bad request, empty string, unknown model, wrong `dimensions`, bad `encoding_format`), 413 (too many inputs, or payload too large), 429 (saturated), 500 (internal). Never a stack trace in a body, and never an unhandled Lambda `FunctionError` for any input, however malformed. The invoke itself must always return the envelope.

**Capacity and batching.**

- The server must accept at least 128 inputs per request and answer more than it will accept with 413.
- graph-rag clients send at most 64 inputs per request AND at most 5 MiB of serialized event per invoke (the synchronous invoke payload limit is 6 MB; 5 MiB leaves headroom). A single input that cannot fit under that bound is refused client-side before sending. A server that receives an event above its own limit must answer 413, not 5xx.
- graph-rag fans out several invokes concurrently (bounded) to use more than one container. The server must be safe to run as many concurrent containers.
- Clients never send empty strings (graph-rag zero-fills empty inputs client-side, matching its OpenAI provider), but the server answers an empty string with 400, not 5xx.

## Numerics and release identity

**Model.** `sentence-transformers/all-MiniLM-L6-v2`: BERT, WordPiece, uncased (`do_lower_case` true), 384 dimensions, mean pooling over the attention mask, then L2 normalization. The artifact is imported once with:

```bash
apr import <hf snapshot>/model.safetensors --arch bert -o minilm.apr --allow-no-config --offline
```

The tokenizer is supplied separately as `tokenizer.json` (spike 009 did this). The import prints "Grade F ... 71 of 75 checks not implemented"; that is a warning, not a failure.

**Parity acceptance on your side.** Cosine >= 0.9999 against sentence-transformers run with `max_seq_length = 256` over a fixed text set. Spike 009 measured >= 0.999998 on 79 real chunks once truncation was aligned, so this is a floor you should clear with room to spare.

**The embedder's identity is a release bundle, not a model name.** The model name and the weights alone do not identify the embedder: a changed tokenizer asset or encoder config moves vectors while leaving the name untouched. Each release you deliver includes all of:

| Field | What |
|-------|------|
| release tag | the aprender release the function was built from |
| deployed `CodeSha256` | as reported by `aws lambda get-function` for the delivered function |
| `.apr` sha256 | the final imported artifact |
| source safetensors sha256 | `model.safetensors` before import |
| `tokenizer.json` sha256 and `vocab.txt` sha256 | the tokenizer assets in use |
| Hugging Face revision | the commit of `sentence-transformers/all-MiniLM-L6-v2` imported |
| encoder config | `do_lower_case: true`, `max_seq_length: 256`, pooling `mean`, `normalize: true` |
| `aprender-embed-reference-vectors.json` and its sha256 | see "Truncation" below |

The deployed `CodeSha256` may change only through a release. graph-rag records it before and after its proof run and rejects a change in between.

## Truncation (mandatory, server-side)

Truncation is the SERVER's obligation. Every input is cut so that the COMPLETE encoded sequence is at most 256 tokens including the `[CLS]` and `[SEP]` special tokens, before the forward pass. That is sentence-transformers `max_seq_length = 256` semantics, i.e. at most 254 content wordpieces. A 1,609-wordpiece code or table chunk returns 200; it never panics and never returns 5xx.

graph-rag relies on this to make build-time truncation and query-time truncation identical by construction, without a tokenizer on the client.

**Boundary texts (normative definitions).** In the uncased vocabulary `the` is a single wordpiece. Texts are space-joined words.

- Word list A (50 words): river mountain forest valley stone cloud winter summer garden bridge ocean island desert meadow harbor lantern orchard canyon glacier prairie thunder autumn spring breeze shadow harvest village castle tower window mirror candle anchor compass feather violin ribbon basket blanket pillow kettle hammer ladder pencil wagon saddle fabric marble copper silver
- Word list B (50 words): budget profit revenue server network thread buffer socket packet router backup cluster signal sensor battery circuit voltage current engine pump valve wheel rocket orbit radio satellite salary audit tax loan share auction contract deposit premium currency ledger invoice database kernel program delay capacity software upgrade code protocol pipeline reactor market
- 300-word tail A / tail B: word list A / B repeated six times. 50-word tail A / tail B: word list A / B once.
- **T-UPPER-A** = `the` repeated 260 times, then 300-word tail A. **T-UPPER-B** = the same 260 `the`, then 300-word tail B. A server truncating at 256 returns the same vector for both (the tails are cut away); one truncating at 512, or not at all, does not.
- **T-LOWER-A** = `the` repeated 200 times, then 50-word tail A. **T-LOWER-B** = the same 200 `the`, then 50-word tail B. Total 250 content wordpieces, under the cap, so a server truncating at 256 returns clearly different vectors; one truncating at 128 or 200 does not.
- **T-253**, **T-254**, **T-255** = `the` repeated 253, 254 and 255 times. T-253 and T-254 fit under the 256-token cap (253 + 2 and 254 + 2); T-255 is one wordpiece over (255 + 2 = 257) and is cut to 254.

If any listed word turns out not to be a single wordpiece in the delivered `tokenizer.json`, report that back to graph-rag as a defect in this brief; do not silently substitute words, because graph-rag builds the T-UPPER and T-LOWER texts from these definitions.

**Reference vectors.** With each release deliver `aprender-embed-reference-vectors.json`:

```json
{
  "model": "all-MiniLM-L6-v2",
  "hf_revision": "<commit>",
  "tokenizer_sha256": "<64 hex>",
  "max_seq_length": 256,
  "texts": [ { "id": "T-253", "text": "the the ...", "vector": [ "... 384 floats ..." ] } ]
}
```

computed with sentence-transformers (`max_seq_length = 256`) over every boundary text above (T-UPPER-A, T-UPPER-B, T-LOWER-A, T-LOWER-B, T-253, T-254, T-255). graph-rag's `reference-vectors` check requires the server's vector for each text to be within cosine 0.9999 of the reference.

## Deployment limits and cold start

The 91 MB model forces a deliberate packaging choice. Either a container image, or a zip/layer with the model loaded once per container; you decide and record it, within Lambda's 250 MB unzipped limit for zips and 10 GB for images.

Please produce and report these numbers; do not leave them as intentions:

- memory size and timeout (timeout at least 30 s);
- MEASURED cold-start latency and warm latency, each for 1 input and for 64 inputs (64 inputs of realistic chunk length, not 64 one-word strings);
- provisioned-concurrency guidance (whether it is needed to keep the cold start inside the budget below, and at what cost).

These are availability inputs, not trivia: graph-rag's own cold start now performs one canary embed through this function and refuses to serve if it does not answer within the canary timeout set in its contract (30 s, Plan 205-07). A function whose cold start plus first embed exceeds that makes the graph-rag instance fail to start.

## Deployment and security posture

- No Function URL, no API Gateway or load-balancer integration, and no public ingress at any point.
- The function's resource-based policy (`aws lambda get-policy`) either does not exist, or contains no statement whose Principal is `*` or `{"AWS": "*"}`, no `apigateway.amazonaws.com` or `elasticloadbalancing.amazonaws.com` service principal, and no `lambda:InvokeFunctionUrl` grant. The absence of a Function URL alone is not proof that there is no ingress; the policy must be checked, and graph-rag's live proof (Plan 205-09) checks it on the delivered function.
- Access is only by IAM `lambda:InvokeFunction` scoped to this function's ARN.
- Function name convention: `aprender-embed-endpoint-<stage>`, where the stage comes from the environment roster. This brief contains no account ids or profile names, and neither should the function code or its deploy defaults.
- Deployment is driven by the aprender repository's own just recipes and pinned to a release artifact, following aprender Phase 9's pattern (internal OpenAI-compatible LLM endpoint: deployment pinned to a release artifact that passes a conformance probe).

## Acceptance (what graph-rag will run)

graph-rag's recipe (built in Plan 205-06):

```bash
just aprender-embed-conformance lambda://<function-name>/v1/embeddings <reference-vectors.json>
```

must print `CONFORMANCE RESULT PASS 13/13`. The thirteen named checks:

1. **shape** - count matches input, each embedding has 384 floats, all finite.
2. **l2-normalized** - norm 1 within 1e-3.
3. **non-degenerate** - three different texts, pairwise cosine < 0.99 (fails the zero-vector stub).
4. **deterministic** - the same text twice, cosine >= 0.99999.
5. **batch-order-invariant** - batched result equals the single-input result, cosine >= 0.9999.
6. **long-input** - an input of at least 1,609 wordpieces returns 200.
7. **truncation-upper** - T-UPPER-A vs T-UPPER-B, cosine >= 0.9999.
8. **truncation-lower** - T-LOWER-A vs T-LOWER-B, cosine < 0.999.
9. **reference-vectors** - every boundary text within cosine 0.9999 of the delivered reference vector.
10. **semantic-sanity** - three query/relevant/irrelevant triples order correctly.
11. **model-echo** - the response `model` equals `all-MiniLM-L6-v2`.
12. **empty-string-400** - an empty-string input returns 400 with the OpenAI error envelope.
13. **wrong-dimensions-400** - `dimensions` other than 384 returns 400.

Run the equivalent against your own build before delivering; the same probe is the acceptance, so there is no second, private standard. After that, graph-rag's live end-to-end test (Plan 205-09) builds a snapshot through the deployed function and answers a query.

## Hand-off

The aprender phase delivers, per environment:

- the function name and region;
- the release bundle from "Numerics and release identity";
- `aprender-embed-reference-vectors.json` and its sha256;
- the deployment-limit numbers from "Deployment limits and cold start".

graph-rag consumes these as configuration only:

```toml
[embedding]
provider = "aprender"
model = "all-MiniLM-L6-v2"
dimension = 384
base_url = "lambda://<function-name>/v1/embeddings"
```

This is a version hand-off, not a path hand-off: graph-rag never reads from or writes into the aprender repository, and the aprender phase changes nothing in the pmcp.run repository.

Out of scope for the aprender phase: any change to the pmcp.run repository. Out of scope for both: multilingual or larger encoders, Cohere (deferred), and migration or re-embedding tooling (D-02).

## Aprender phase id

Opened as: deferred (2026-10-22): the aprender server work is not yet scheduled, so no aprender phase id exists. Re-open by creating the phase in the aprender repository from this brief and replacing this line with its id. Until then graph-rag Plan 205-09 (the Aprender live proof) stays blocked on its precondition; all other Phase 205 plans are unaffected.
