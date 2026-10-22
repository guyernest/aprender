//! `bootstrap` for server id `aprender-mcp-decide-antisemite` (Phase 10 D-19).
//!
//! A shim: the Lambda runtime, the routes and the one `classify` tool live in
//! `aprender-mcp-decide-lambda` and `aprender-mcp-decide`. The package name goes in so the
//! health body and the binary name the crate that was built, not the shared library.

fn main() {
    if let Err(e) = aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME")) {
        eprintln!("{}: {e}", env!("CARGO_PKG_NAME"));
        std::process::exit(1);
    }
}
