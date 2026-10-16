INTERIM build (upstream/main d7ac5f782 + PRs content/alias/auth) -- not the deployment of record. Binary `apr 0.71.3 (bd85d728f)` built natively from upstream/main d7ac5f7829ad1d260f72620a55d29dbbc6a8ba04 plus three cherry-picked draft-PR commits (paiml/aprender#5411 content, #5412 alias, #5451 auth gate; none merged), served on loopback 127.0.0.1:8030 with `APR_API_KEY` set (plaintext key form, hashed in-process). Never deployed, never the pin (D-02); outside the reader's `conformance-*.json` glob.

Produced by `bash deploy/llm-endpoint/probe/auth_probe.sh 8030 "$KEY" --expect gated` (assertion PASSED). The throwaway key is not printed. Contrast with the open release table, `auth-table-v0.71.4-release-local.md`.

| request | HTTP | body |
|---|---|---|
| POST /v1/chat/completions, no Authorization header | 401 | {"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"} |
| POST /v1/chat/completions, Bearer wrong-key | 401 | {"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"} |
| POST /v1/chat/completions, Bearer <the key> | 200 | {"id":"chatcmpl-q4k-1792164390886","object":"chat.completion","created":1792164391,"model":"x","choices":[{"index":0,"me |
| GET /health, no header | 401 | {"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"} |
| GET /v1/models, no header | 401 | {"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"} |

PASS: gated (401 without a valid bearer on every row, 200 with the key)
