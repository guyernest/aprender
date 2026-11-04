//! `aprender-mcp-builtin` — the pmcp.run `aprender` built-in server type, model side.
//!
//! The platform's generated server crate is the only caller. It embeds a `config.toml`,
//! passes the process arguments and a serving closure to [`main_with`], and pins this crate by
//! git revision. Design: `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md`.
//!
//! The binary contract (design §5.1, §6.2):
//!
//! - `["bootstrap"]` and `["bootstrap", "--serve-local"]` build the server and hand it to the
//!   closure (the platform proxy, or a localhost listener; the crate does not care which).
//! - `["bootstrap", "precheck"]` prints the WARN lines and one verdict line to stdout and
//!   exits 0 eligible, 2 refused, 3 config invalid, 4 store unreachable:
//!   `ERROR: REFUSED <code>: <detail>`, `ERROR: REFUSED s3-unreachable: <op> <uri>: <cause>`
//!   or `ELIGIBLE {json}`. No line ever carries a record's contents.
//! - Anything else is a usage error, exit 64.

pub mod config;
pub mod load;
pub mod precheck;
pub mod server;
pub mod store;
pub mod tools;

use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;

// Re-exported at the versions this crate was built with, so the template's closure type and
// the crate's agree on one `pmcp::Server` and one `lambda_http::Error`.
pub use lambda_http;
pub use pmcp;

use crate::precheck::Env;
use crate::store::{ObjectStore, S3Store};

/// A usage error: an argument vector the contract does not name.
pub const EXIT_USAGE: i32 = 64;

/// What the process arguments ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Serve,
    ServeLocal,
    Precheck,
    Usage,
}

/// The argv dispatch table of the binary contract.
#[must_use]
pub fn dispatch(args: &[String]) -> Mode {
    match args {
        [_] => Mode::Serve,
        [_, flag] if flag == "--serve-local" => Mode::ServeLocal,
        [_, sub] if sub == "precheck" => Mode::Precheck,
        _ => Mode::Usage,
    }
}

/// The future the template's serving closure returns.
pub type ServeFuture =
    Pin<Box<dyn Future<Output = Result<(), lambda_http::Error>> + Send + 'static>>;

/// Run the precheck and write its output (design §5.2) to `out`; returns the exit status.
pub async fn run_precheck(
    toml_text: &str,
    expected_id: &str,
    env: &Env,
    store: &dyn ObjectStore,
    out: &mut dyn Write,
) -> i32 {
    let outcome = precheck::run(toml_text, expected_id, env, store).await;
    // A broken stdout cannot be reported anywhere; the exit status still carries the verdict.
    let _ = out.write_all(outcome.render().as_bytes());
    let _ = out.flush();
    outcome.exit
}

/// The one binary entry point the template calls (design §6.2).
///
/// `args` are the process arguments; `toml_text` the embedded configuration; `expected_id`
/// the template's `SERVER_ID`; `serve` starts the server the way the template chooses.
/// `precheck` and a usage error end the process with their exit status and never return.
///
/// # Errors
///
/// A `lambda_http::Error` when the configuration does not parse (which `precheck` refuses
/// before anything deploys, so a deployed build never sees it), when the server cannot be
/// built, or whatever `serve` returns.
pub async fn main_with(
    args: Vec<String>,
    toml_text: &'static str,
    expected_id: &str,
    serve: impl FnOnce(pmcp::Server) -> ServeFuture,
) -> Result<(), lambda_http::Error> {
    match dispatch(&args) {
        Mode::Usage => {
            eprintln!("usage: bootstrap [--serve-local | precheck]");
            std::process::exit(EXIT_USAGE);
        }
        Mode::Precheck => {
            let store = S3Store::from_default_config().await;
            let env = Env::from_env();
            let code =
                run_precheck(toml_text, expected_id, &env, &store, &mut std::io::stdout()).await;
            std::process::exit(code);
        }
        Mode::Serve | Mode::ServeLocal => {
            let cfg = config::parse(toml_text, expected_id)
                .map_err(|e| lambda_http::Error::from(e.to_string()))?;
            let build_id = std::env::var("PMCP_BUILD_ID")
                .ok()
                .filter(|s| !s.is_empty());
            let store: Arc<dyn ObjectStore> = Arc::new(S3Store::from_default_config().await);
            let server = server::build(
                toml_text,
                cfg,
                expected_id,
                env!("CARGO_PKG_VERSION"),
                build_id,
                store,
            )
            .map_err(|e| lambda_http::Error::from(e.to_string()))?;
            serve(server).await
        }
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! / schema_for! expand to .unwrap()
mod tests;
