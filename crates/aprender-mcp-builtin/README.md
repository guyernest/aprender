# aprender-mcp-builtin

The core crate of the pmcp.run `aprender` built-in server type, model side. The platform's
generated server crate embeds a `config.toml`, calls `aprender_mcp_builtin::main_with` with
the process arguments and a serving closure, and pins this crate by git revision.

- `bootstrap` serves `classify`, `status` and `verify` over one lazily loaded `.apr` decide model.
- `bootstrap --serve-local` serves the same server on localhost through the closure the template passes.
- `bootstrap precheck` refuses an ineligible model at build time (exit 0 eligible, 2 refused, 3 config invalid, 4 store unreachable).

Design: `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md`. Not published:
the `aprender-*` names on crates.io belong to upstream.
