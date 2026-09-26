# aprender-mcp-decide

A **thin, single-model MCP server** for decision models: one verified
`decide-apr-v1` artifact (a Laya ModernBERT decision model) behind ONE `classify`
tool, built on [pmcp](https://github.com/paiml/rust-mcp-sdk). The tool is bound to
the artifact's own task — its question and its ordered labels come from the
artifact, never from the caller (Phase 8 D-09). It copies the
`aprender-mcp-setfit` template: transport only, in-process, bounds owned by the
transport and named from a contract.

Part of the [aprender](https://github.com/paiml/aprender) monorepo.

## Run locally (stdio)

```bash
cargo run -p aprender-mcp-decide -- --model models/decide/<model>.apr
APRENDER_DECIDE_MODEL=models/decide/<model>.apr cargo run -p aprender-mcp-decide
```

Register it in an MCP client (Claude Desktop, Claude Code, Cursor) as a stdio
server with the same arguments. Everything human-readable goes to stderr; stdout
belongs to the protocol. This runner reads a LOCAL path only — the pmcp.run
deployment is `aprender-mcp-decide-lambda`, which fetches the artifact from S3.

The server advertises exactly one tool:

| Tool | Arguments | Returns |
|------|-----------|---------|
| `classify` | `texts: [string]` — 1..=8 texts, each at most 16384 UTF-8 bytes, at most 1024 model tokens over the request. Nothing else (`deny_unknown_fields`). | `model` {`artifact_sha256`, `recipe_id`, `method`, `base`}, `labels` (task order), and `results`, one per text in input order: `label`, `probabilities` (calibrated, one per label in `labels` order — an array, never a map), `tokens`, `truncated`. |

The tool description is built from the artifact: its question, its labels in
order, and the bounds. Each element of `texts` is ONE complete document; a text
longer than the model's window is truncated by the model and flagged
`truncated: true` (D-12).

## Bounds (`contracts/decide-tool-boundary-v1.yaml`)

- **On the async handler path** (`precheck`, which takes no model and so cannot
  tokenize): the text count, then each text's UTF-8 byte length.
- **Inside one admitted blocking section** (`classify_blocking`): tokenize once,
  check the sum of built-row tokens against the budget, then score.
- **Admission, per process:** at most `classify_max_in_flight` (1) computation runs
  and at most `classify_max_pending` (4) requests are admitted; the next is refused
  at once. A slot is released only when its blocking work ends, so a disconnected
  caller cannot free CPU that is still being spent.

Every refusal is a validation error naming the contract key and the observed
value, and never echoes the caller's text. `ClassifyLimits::CONTRACTED` is
asserted equal to the contract by a unit test.

## Tests

```bash
cargo test -p aprender-mcp-decide --lib               # bounds, admission, shape, contract mirror
cargo test -p aprender-mcp-decide --test e2e_stdio    # live stdio on the packed tiny fixture
APR_MCP_E2E_DECIDE_MODEL=$PWD/models/decide/<model>.apr \
  cargo test -p aprender-mcp-decide --test e2e_stdio  # plus the real-model leg
```
