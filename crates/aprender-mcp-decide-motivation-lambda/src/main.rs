//! `bootstrap` for server id `aprender-mcp-decide-motivation` (Phase 12 D-02).
//!
//! A shim: the Lambda runtime, the routes and the one `classify` tool live in
//! `aprender-mcp-decide-lambda` and `aprender-mcp-decide`. The package name goes in so the
//! health body and the binary name the crate that was built, not the shared library. It serves
//! the motivation-v1 PILOT artifact (a three-label model trained on the 25 October 2026 draft
//! of the classification guide); the register growth it needs is the owner's recorded Phase 12
//! D-02 decision.

fn main() {
    if let Err(e) = aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME")) {
        eprintln!("{}: {e}", env!("CARGO_PKG_NAME"));
        std::process::exit(1);
    }
}
