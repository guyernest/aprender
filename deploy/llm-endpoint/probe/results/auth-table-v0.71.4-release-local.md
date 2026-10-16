RELEASE pin v0.71.4 (`apr 0.71.4 (5027da624)`, image `aprender-llm-endpoint:cpu-v0.71.4`), run locally under `docker --memory 10g`, loopback port 8029 -- this is the release as shipped: there is NO server-side auth gate on the GGUF path at v0.71.4, which is the reason the D-04 PR exists (paiml/aprender#5451). The container was started with `APR_API_KEY_HASH` set to a valid 64-hex hash and still answered every row 200: the hash is read by the entrypoint but never enforced by this release's GGUF router. Not the deployment of record; not a record the reader reads (outside the `conformance-*.json` glob).

Produced by `bash deploy/llm-endpoint/probe/auth_probe.sh 8029 "$KEY" --expect open` (assertion PASSED). The throwaway key is not printed.

| request | HTTP | body |
|---|---|---|
| POST /v1/chat/completions, no Authorization header | 200 | {"id":"chatcmpl-q4k-1792163698807","object":"chat.completion","created":1792163700,"model":"x","choices":[{"index":0,"me |
| POST /v1/chat/completions, Bearer wrong-key | 200 | {"id":"chatcmpl-q4k-1792163700481","object":"chat.completion","created":1792163701,"model":"x","choices":[{"index":0,"me |
| POST /v1/chat/completions, Bearer <the key> | 200 | {"id":"chatcmpl-q4k-1792163701431","object":"chat.completion","created":1792163702,"model":"x","choices":[{"index":0,"me |
| GET /health, no header | 200 | {"status":"ok","version":"0.71.4","compute_mode":"cpu","model_loaded":true,"uptime_sec":10.658950214} |
| GET /v1/models, no header | 200 | {"object":"list","data":[{"id":"default","object":"model","created":1792163691,"owned_by":"realizar"}]} |

PASS: open (every row is 200: no auth at the server)
