# deploy/embed-endpoint

Phase 11 deployment tooling for the aprender embeddings endpoint: an
OpenAI-compatible `POST /v1/embeddings` served by a Lambda container, reachable
only through IAM-invoke. This directory holds its events, its recipes and the
runbook skeleton. Nothing here is a promise: every number a plan reports is a
measurement taken by that plan, and a later plan that re-measures supersedes it.

## Layout

| Path | Purpose |
|------|---------|
| `events/` | Credential-free API Gateway v2 events for `POST /v1/embeddings`, shared by every later plan |
| `graviton-probe/` | The standalone Graviton probe and its arm64 asset recipe (plan 11-01) |
| `.ids/` | **Gitignored.** Per-checkout identifiers (account, region, function, URL). Account and region live only here, never in a committed file |

## Events

| File | Carries |
|------|---------|
| `events/embeddings-single.json` | model `sentence-transformers/all-MiniLM-L6-v2`, one input: `The cat sat on the mat.` |
| `events/embeddings-three.json` | the same model, three inputs: the three texts of `crates/aprender-core/tests/fixtures/setfit/full_model_reference.json` |

Both follow the shape of `deploy/llm-endpoint/events/apigw-v2-no-auth.json`
(version 2.0, `routeKey` `$default`, `requestContext.http.path` `/v1/embeddings`,
`isBase64Encoded` false) and contain no address, identifier, account or ARN.

## Driving the real binary: `scripts/embed_endpoint/mock_runtime.py`

The driver serves the Lambda Runtime API locally and spawns a real binary
exactly as Lambda would. It is standard library only, so run it with `-I`.

```bash
# the driver's own case table (14 cases; a stand-in client)
python3 -I scripts/embed_endpoint/mock_runtime.py --selftest

# one JSON event file, re-serialized; one raw file, sent as its exact bytes
python3 -I scripts/embed_endpoint/mock_runtime.py --bin <binary> \
    --event deploy/embed-endpoint/events/embeddings-single.json \
    --raw-event <file-with-hostile-bytes> --out <dir>
```

The CLI prints one JSON record per payload and exits 0 only when every record
is a `response`. A record has `kind` (`response`, `error`, `init_error` or
`no_answer`), `body`, `error_type` and `request_id`.

From Python, `invoke_many(binary, events, env)` runs a finite batch, and
`Session(binary, env)` keeps one child alive across many `invoke(event)` calls.
Plan 11-11's harness uses `Session`. Plans 11-06, 11-07 and 11-11 import these
with an explicit `sys.path` insert of `scripts/embed_endpoint/`.

## Status

| Item | Plan | Status |
|------|------|--------|
| `scripts/embed_endpoint/` probes and packaging (`glibc_floor.py`, `graviton_gate.py`, `graviton_run.py`, `ingress_scan.py`, `package_zip.py`) | 11-01 | exists |
| `deploy/embed-endpoint/graviton-probe/` | 11-01 | exists |
| `scripts/embed_endpoint/mock_runtime.py` (Runtime API driver, selftest) | 11-14 | exists |
| `events/embeddings-single.json`, `events/embeddings-three.json` | 11-14 | exists |
| local invoke recipes and the outer-event proof | 11-06 | planned |
| `scripts/embed_endpoint/wire_hostile.py` | 11-07 | planned |
| `scripts/embed_endpoint/conformance_local.py` (inproc transport over `Session`) | 11-11 | planned |
