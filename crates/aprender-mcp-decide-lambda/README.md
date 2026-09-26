# aprender-mcp-decide-lambda

The AWS Lambda custom-runtime wrapper around `crates/aprender-mcp-decide` (Phase 8 D-15).
It adds no tool, no bound and no inference path: the server crate owns the one `classify`
tool, its bounds (`contracts/decide-tool-boundary-v1.yaml`) and the decide-apr-v1 load
doors, and this crate is a transport shim in front of them.

The Custom Runtime API requires the executable to be named `bootstrap`, so that is the
binary this crate produces. Each invocation is proxied over loopback to an in-process
pmcp streamable-HTTP server configured `stateless()`. That is the only mode that survives
serverless: API Gateway routes successive requests to different containers, so a cold
container can receive a `tools/call` whose `initialize` landed on another one. A loopback
test proves the stateless server answers exactly that request.

## Where the weights come from (D-18)

| Variable | Meaning |
|---|---|
| `APRENDER_DECIDE_S3_URI` | `s3://bucket/key` of the served `.apr` (the Lambda source) |
| `APRENDER_DECIDE_SHA256` | the artifact's sha256, 64 lowercase hex characters. Required with the S3 URI |
| `APRENDER_DECIDE_MODEL` | a local `.apr` path (local runs), optionally pinned by `APRENDER_DECIDE_SHA256` |
| `PMCP_SERVER_ID` | server name (default `aprender-decide`) |

Setting both sources is refused, as is setting neither. The S3 layout is content-addressed:
`s3://<bucket>/decide/<server>/<sha256>.apr`, so the key and the pin name the same bytes.

**The weights are never baked into the package.** Spikes 021 and 026 measured baked weights
7-18x worse on cold start than an S3 fetch at 10,240 MB. At cold start the object is
fetched by 16-way 64 MiB ranged GETs, with 5 attempts per part (8 s each) and a 25 s
overall deadline, **straight into one pre-sized in-memory buffer**. Lambda's scratch disk
is 512 MB and cargo-pmcp cannot raise it, so a 0.85 GB artifact cannot be staged there.
A length over the decide-apr-v1 cap is refused before the buffer is allocated.

The buffer's sha256 must equal the pin **before the load ladder parses a byte**. The pin is
both the tamper guard and the identity every `classify` response reports as
`model.artifact_sha256`.

## Lazy load and cold-start evidence

The model loads on the first MCP request, behind a lock, so concurrent first calls load
once. It does not load in the init phase, because default Lambda caps init near 10 s. A
failed load returns HTTP 503 with only the failure kind (the detail goes to the log), and
the next request retries.

The request that performs the load logs
`decide.load performed_load=true probe_id=<id> load_ms=<n> download_ms=<n> build_ms=<n> graviton=<gen>`.
Every other request logs `performed_load=false`. Every proxied response carries
`x-decide-load: cold;load_ms=<n>` or `warm`. The probe id comes from the optional
`x-decide-probe-id` request header, accepted only as at most 64 `[A-Za-z0-9-]` characters.
Request text is never logged.

## Probe

`examples/probe.rs` checks a live (or local) endpoint against a local copy of the
artifact. It checks for exactly one `classify` tool, the labels in task order and the
served sha256. With `--maximal concentrated|distributed --cold-first` it sends the maximal
legal request as the first and only POST, for the accepted-region test.

```bash
cargo run -p aprender-mcp-decide-lambda --example probe -- \
    --url https://<endpoint>/ --apr model.apr [--expect-sha256 <hex>] \
    [--maximal concentrated|distributed] [--cold-first] [--probe-id <id>]
```

A bearer token is read from `--bearer` or `APRENDER_DECIDE_PROBE_TOKEN`, and it is never
printed.

## Build

```bash
cargo test -p aprender-mcp-decide-lambda --lib
ulimit -n 65536
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.34 \
    -p aprender-mcp-decide-lambda --bin bootstrap
```

The deploy config template is `.pmcp/deploy.toml.template`. The generated `deploy.toml`
is gitignored, because the bucket name carries the AWS account id.

Part of the [aprender](https://github.com/paiml/aprender) monorepo.
