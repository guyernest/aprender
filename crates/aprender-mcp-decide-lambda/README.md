# aprender-mcp-decide-lambda

The AWS Lambda custom-runtime wrapper around `crates/aprender-mcp-decide` (Phase 8 D-15).
It adds no tool, no bound and no inference path: the server crate owns the one `classify`
tool, its bounds (`contracts/decide-tool-boundary-v1.yaml`) and the decide-apr-v1 load
doors, and this crate is a transport shim in front of them.

This crate is a model-agnostic **library** (since D-19): the runtime is `bootstrap::run`, and
each served model has its own thin `aprender-mcp-decide-<dataset>-lambda` crate that calls it
(see "One server id, one thin crate"). The Custom Runtime API requires the executable to be
named `bootstrap`, so that is the binary each thin crate produces. Each invocation is proxied over loopback to an in-process
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
fetched by 16-way 64 MiB ranged GETs, with 5 attempts per part (8 s each) and a 10.739 s
overall deadline, **straight into one pre-sized in-memory buffer**. That deadline is the 30 s
gateway cap minus the post-download work measured on the 10,240 MB tier (sha256 + load ladder
+ the largest classify, 15.261 s) and the contract's 4 s margin. The download is also capped at
the invocation's remaining time minus that 19.261 s reserve. Lambda's scratch disk
is 512 MB and cargo-pmcp cannot raise it, so a 0.85 GB artifact cannot be staged there.
A length over the decide-apr-v1 cap is refused before the buffer is allocated.

The buffer's sha256 must equal the pin **before the load ladder parses a byte**. The pin is
both the tamper guard and the identity every `classify` response reports as
`model.artifact_sha256`.

## Lazy load and cold-start evidence

The model loads on the first MCP request (a `POST`) through a tokio `OnceCell`, so
concurrent first calls share one load. No other method loads: `GET` answers the health
body, `OPTIONS` the CORS preflight, and anything else is a 405. The health body says
`ok: true` only when the model-source config parses; a missing or malformed
`APRENDER_DECIDE_*` config answers 503 with `ok: false` and `config_error`. It does not
load in the init phase, because default Lambda caps init near 10 s. A failed load returns
HTTP 503 with only the failure kind (the detail goes to the log), and the next request
retries. If the in-process MCP server task ever ends, the process exits so Lambda
replaces the environment.

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
    [--maximal concentrated|distributed] [--cold-first] [--probe-id <id>] \
    [--replay N [--replay-from K]] [--replay-over-limit]
```

`--replay N` is the acceptance replay: one bare `tools/call` of N committed synthetic texts (N from 1 to
the compiled maximum), checked for one result per text, probabilities that index the labels in task order
and sum to one, and the served sha256. `--replay-from K` starts the cycle of texts at the K-th, so several
one-text calls need not repeat one request. `--replay-over-limit` sends the compiled maximum plus one text
and passes only when the server refuses it, naming the bound. Timings are reported, never gated.

A bearer token is read from `--bearer` or `APRENDER_DECIDE_PROBE_TOKEN`, and it is never
printed.

## Deployed

Live on pmcp.run as **`aprender-mcp-decide-antisemite`** since 2026-10-23 (plan 10-12), on the 10,240 MB
tier, serving `antisemitism-v1`. The outcome is `verified`. The record is
`.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-RENAME-DEPLOY-EVIDENCE.json`.
The artifact first went live on 2026-10-22 under the previous server id `aprender-mcp-decide` (plan 10-08; that id had served since 2026-09-27, on 10,240 MB since 2026-09-28), which the owner retired on 2026-10-23 (D-19, D-20).

The previous endpoint https://aprender-mcp-decide.us-east.true-mcp.com/mcp was retired by the owner on 2026-10-23 and answers "Server not found"; clients use the endpoint below and pin it with the sha256 in the table.

| | |
|---|---|
| Endpoint | `https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp` (POST only) |
| Auth | Off, by the owner's decision (D-13; the cost risk was accepted). No token or header is needed |
| Tier | 10,240 MB, arm64 |
| Task | Antisemitism in tweets (GoldStandard2024; the Jikeli et al. annotation guide condensed as the task), labels `not_antisemitic`, `antisemitic` (in task order) |
| Model | `laya-antisemitism-v1.apr`, sha256 `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e` |

The stance demo (`laya-stance-64.apr`, sha256 24a44d7e050166c9b64e2716f2bcb3ce91747f7a3b927d03d6eeae5f89b6275a)
is no longer served since 2026-10-22 (D-13): the first deployment of this artifact replaced it in place.

### Connecting an MCP client

POST JSON-RPC to the endpoint. The server is stateless:
- a bare `tools/call` works with no `initialize`;
- a full client (`initialize` → `notifications/initialized` → `tools/list` → `tools/call`) works too.

`GET /mcp` is answered by the pmcp.run edge with 405, not by this server. `GET /health` is the
platform's answer, and it does not reflect whether MCP calls succeed.

```
POST https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp
content-type: application/json
accept: application/json, text/event-stream

{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"classify","arguments":{"texts":["The weather today is mild and sunny."]}}}
```

The example text is a synthetic sentence, not a tweet. The tool result text is JSON:
- `model`: `artifact_sha256`, `recipe_id`, `method`, `base`;
- `labels`, in task order;
- one entry in `results` per text: `label`, `probabilities` in label order, `tokens`, `truncated`.

The probabilities this sentence returns are not quoted here: no live replay of it has been recorded yet
(the acceptance replay is plan 10-09, which had not run when this was written). `examples/probe.rs --replay 1`
produces them.

### Identity check

Every response carries `model.artifact_sha256`. It must equal
`2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`, the pin the function
verifies before it parses a byte. `examples/probe.rs --expect-sha256 <that>` checks it, together
with the single `classify` tool and the label order.

### Limits on the 10,240 MB tier

The door refuses anything over these limits (`contracts/decide-tool-boundary-v1.yaml`):
- **8 texts** or fewer per call.
- **800 built tokens** or fewer per call.
- **Cold call: 19.8-25.5 s** at the client, over 4 proven-cold samples of the two maximal
  requests (concentrated: 2 texts; distributed: 8 texts; 800 built tokens each). The first call after idle loads 846 MB
  from S3 (9.0-9.2 s of it). The worst sample left 4.50 s under the 30 s gateway cap.
- **Warm call: 1.08-1.15 s** (5 calls).

The stance deployment measured 22.6-27.0 s cold and 0.87-0.98 s warm; the figures above are this
deployment's own, reported from `10-RENAME-DEPLOY-EVIDENCE.json` and not gated (D-16).

### Claim scope

The served model passed laya-finetune-gate-v1 (`10-GATE-RUN-EVIDENCE.json`, plan 10-04). On the 2,129 held-out
GoldStandard2024 rows it scored macro-F1 0.7956 against 0.5596 zero-shot (margin 0.236 against the 0.05 bar)
with post-calibration ECE 0.0424 against the 0.10 bar, the median-ECE seed (23) of 13, 17 and 23. The gate's
claim (`eval_set.claim`) is:

> The gate now certifies margin and calibration on held-out data drawn like the tenant's shots.
> It does NOT certify robustness to a shifted input population.

It is in-distribution calibration, not shift robustness, and no shift probe was measured for this model
(`shift_probe` is null in the record). The SemEval-2016 shift probe (ece_post 0.1896) belongs to the retired
stance demo.

No SetFit comparison number exists for this task. Plan 10-05 built the SetFit cell but did not run it
(`apr eval` exceeded the host's memory on the validation split), the owner held that success criterion (SC-4)
open, and the follow-on plan 10-06 is deferred by the owner. Nothing here claims how this model compares with SetFit.

## One server id, one thin crate

A served model is a server id named for its dataset, `aprender-mcp-decide-<dataset>` (the first
is `aprender-mcp-decide-antisemite`).
One server id has one thin crate named `<id>-lambda`, and that crate is a shim over this one: its `main.rs` calls `bootstrap::run` and nothing else.
The server id equals the crate name minus `-lambda`, because `cargo pmcp deploy` resolves a server id to
`<deploy-root>/<server>-lambda` before any other search. `tests::thin_server_crates_are_shims` fails a
thin crate that grows a handler, a route, a tool or a second dependency.

To add a model:

1. Create the thin crate: `Cargo.toml` (a `bootstrap` bin and this crate as its only dependency),
   a shim `main.rs`, and a `README.md`.
2. Add it to the workspace, and swap or grow the deployment-unit register FALSIFY-MONO-011 in
   `crates/aprender-core/tests/monorepo_invariants.rs` (and the `bootstrap` line of
   `scripts/duplicate_bin_names_allowlist.txt`). The first thin crate swapped the shared crate's
   entry (9 to 9). Every further thin crate is growth and needs the owner's recorded CONTEXT
   decision with a `blocking-human` checkpoint, never a same-PR edit.
3. Run `just laya-resolver-proof <sdk> <commit> <scratch> <server>` for the new id.
4. Deploy through the recipes with that id: `laya-deploy-config`, `laya-upload`, `laya-deploy`,
   `laya-grant`, `laya-deploy-verify`. The id is a required argument of each, with no default.

Servers SERVE in parallel, but DEPLOYS run one at a time. The shared crates root's `.pmcp/` and
`deploy/` belong to the setfit training server. `_laya-crates-root-swap` installs one config alone
and restores the rest byte-identically, and it refuses while a leftover backup exists, so a second
concurrent deploy fails closed.

## Build

```bash
cargo test -p aprender-mcp-decide-lambda --lib
ulimit -n 65536
just laya-build-bootstrap <server>   # e.g. aprender-mcp-decide-antisemite
# which runs: cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.34 \
#     -p <server>-lambda --bin bootstrap
```

The `bootstrap` bin now comes from the thin crate, not from this one. The deploy config template
is `.pmcp/deploy.toml.template`. `just laya-deploy-config` writes the generated
`models/decide/deploy-config-<server>.toml` from it, one file per server id, gitignored because
the bucket name carries the AWS account id.

Part of the [aprender](https://github.com/paiml/aprender) monorepo.
