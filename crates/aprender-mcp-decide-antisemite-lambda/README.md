# aprender-mcp-decide-antisemite-lambda

The Lambda `bootstrap` of the decide MCP server whose server id is
**`aprender-mcp-decide-antisemite`**. It serves the model or models trained on its dataset
(the antisemitism classification data), through the one task-bound `classify` tool.

This crate is a shim and nothing else. Its `main.rs` calls `bootstrap::run` of the shared,
model-agnostic runtime in [`aprender-mcp-decide-lambda`](../aprender-mcp-decide-lambda), which
owns the loopback proxy, the lazy S3 load and the cold-start evidence. The tool surface and
the bounds live in `aprender-mcp-decide` and `contracts/decide-tool-boundary-v1.yaml`.

Why a crate per server id: `cargo pmcp deploy` resolves a server id to
`<deploy-root>/<server>-lambda` first, so a server id needs a crate of exactly that name
(`aprender-mcp-decide-antisemite` + `-lambda`). Several classifiers from different datasets
serve in parallel, each under its own id and its own thin crate; see the shared crate's README,
section "One server id, one thin crate".

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.34 \
    -p aprender-mcp-decide-antisemite-lambda --bin bootstrap
```

Part of the [aprender](https://github.com/paiml/aprender) monorepo.
