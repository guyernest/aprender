# aprender-mcp-decide-motivation-lambda

The Lambda `bootstrap` of the decide MCP server whose server id is
**`aprender-mcp-decide-motivation`**. It serves the motivation-v1 **pilot** artifact, a
three-label model (`social_affiliative`, `conspiratorial`, `other`) trained on the partner's
motivation dataset against the 25 October 2026 draft of the classification guide, through the
one task-bound `classify` tool. The pilot is expected to be retrained once the guide is frozen.

This crate is a shim and nothing else. Its `main.rs` calls `bootstrap::run` of the shared,
model-agnostic runtime in [`aprender-mcp-decide-lambda`](../aprender-mcp-decide-lambda), which
owns the loopback proxy, the lazy S3 load and the cold-start evidence. The tool surface and
the bounds live in `aprender-mcp-decide` and `contracts/decide-tool-boundary-v1.yaml`.

One server id, one thin crate: `cargo pmcp deploy` resolves a server id to
`<deploy-root>/<server>-lambda` first, so a server id needs a crate of exactly that name
(`aprender-mcp-decide-motivation` + `-lambda`). Several classifiers from different datasets
serve in parallel, each under its own id and its own thin crate; see the shared crate's README,
section "One server id, one thin crate". This crate is a deployment unit added under the
owner's recorded Phase 12 decision D-02.

```bash
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.34 \
    -p aprender-mcp-decide-motivation-lambda --bin bootstrap
```

Part of the [aprender](https://github.com/paiml/aprender) monorepo.
