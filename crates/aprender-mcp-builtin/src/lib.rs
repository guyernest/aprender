//! `aprender-mcp-builtin` — the pmcp.run `aprender` built-in server type, model side.
//!
//! The platform's generated server crate is the only caller. It embeds a `config.toml`,
//! passes the process arguments and a serving closure to `main_with`, and pins this crate by
//! git revision. Design: `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md`.

pub mod config;
pub mod precheck;
pub mod store;

#[cfg(test)]
mod tests;
