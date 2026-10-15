| request | HTTP | body |
|---|---|---|
| no Authorization header | 200 | {"id":"chatcmpl-q4k-1792082537083","object":"chat.completion","created":1792082537,"model":"x","choices":[{"index":0,"me |
| Bearer wrong-key | 200 | {"id":"chatcmpl-q4k-1792082537711","object":"chat.completion","created":1792082538,"model":"x","choices":[{"index":0,"me |
| Bearer <correct key> | 200 | {"id":"chatcmpl-q4k-1792082538317","object":"chat.completion","created":1792082538,"model":"x","choices":[{"index":0,"me |
| GET /health, no header | 200 | |
| GET /v1/models, no header | 200 | |
