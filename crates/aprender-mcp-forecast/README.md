# aprender-mcp-forecast

A **thin, single-purpose MCP server**: one stateless `forecast` tool over
[`aprender-forecast`](../aprender-forecast), built on
[pmcp](https://github.com/paiml/rust-mcp-sdk) and deployable to pmcp.run. Part of the
[aprender](https://github.com/paiml/aprender) monorepo; it follows the
`aprender-mcp-setfit` template — one model per server, transport only, no numerics.

The request carries the series; the server fits and forecasts inside the call. There is
no model to train first and no artifact to store.

## Run it

```bash
# stdio — what Claude Code / Claude Desktop / Cursor spawn directly
aprender-mcp-forecast

# loopback HTTP: same-origin demo page at / and MCP streamable-http at /mcp
aprender-mcp-forecast --http 8765

# timing table on synthetic series (a report, not a protocol path)
aprender-mcp-forecast --bench 1000 3000
```

Everything human-readable goes to **stderr**: stdout belongs to the protocol. `--http`
binds `127.0.0.1` only, with `AllowedOrigins::localhost()` — a dev/loopback surface, not
an exposed one.

## The tool

`forecast` takes parallel `ds` (dates, `YYYY-MM-DD`) and `y` arrays plus `horizon`, and
optionally `freq` (`D`/`W`/`MS`), `model`, `growth`, `cap`, `seasonality_mode`,
`interval_width`, `holidays`, `n_lags` and `seed`. It returns future `ds`, `yhat`,
`yhat_lower`, `yhat_upper`, `trend`, named `components`, `fit_seconds`,
`predict_seconds` and `diagnostics`.

The advertised schema is strict (`additionalProperties: false`), so an unknown key is
**refused, not ignored**.

## Read the bands as measured, not as nominal

**The nominal 80 % interval covered 0.60 (Prophet) and 0.66 (NeuralProphet-lite) of
held-out points across 17 rolling-origin windows** (spike 006). The tool description says
so too, because a client that trusts the label rather than the measurement will
under-estimate its own risk.

## What it refuses

Every bound lives in the library, not here — the transport re-checks nothing, so a direct
library caller and an MCP client get identical answers. Refusals: fewer than 10 or more
than 20 000 points; a horizon outside 1…3 650; mismatched `ds`/`y` lengths; a date that
is not exactly ten ASCII bytes of `YYYY-MM-DD`, or is not a real calendar date; `ds` not
strictly ascending; non-finite or constant `y`; a `freq` other than `D`, `W` or `MS`;
`interval_width` outside (0, 1); logistic growth without a `cap`, or a `cap` not
exceeding `max(y)`.

`model: "neuralprophet"` currently refuses with a message naming plan 06-04, which ports
it.

## Deployment

The Lambda wrapper crate is **deferred**: each one adds a `bootstrap` `[[bin]]` to the
`FALSIFY-MONO-011` allowlist count, and cargo-pmcp's `*-lambda` discovery is already
ambiguous. `http_app` and `StreamableHttpServerConfig::stateless()` ship here so that
wrapper is a short copy when it is wanted.
