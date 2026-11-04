//! The platform template's `main.rs` shape, for `just builtin-smoke`: the same `main_with`
//! call the generated server makes, with the serving closure replaced by the fork's
//! loopback listener for both modes (there is no Lambda here). The configuration is read at
//! runtime from `APRENDER_SMOKE_CONFIG` (the template embeds it with `include_str!`).
//!
//! `local_server precheck` and `local_server --serve-local` are what the smoke drives.

use std::net::SocketAddr;

use aprender_mcp_builtin::lambda_http::Error;

const SERVER_ID: &str = "aprender-seed";
const LOCAL_ADDR: &str = "127.0.0.1:8081";

async fn serve_local(server: aprender_mcp_builtin::pmcp::Server) -> Result<(), Error> {
    let addr: SocketAddr = LOCAL_ADDR
        .parse()
        .map_err(|e: std::net::AddrParseError| Error::from(e.to_string()))?;
    let (bound, handle) = aprender_mcp_decide_lambda::start_loopback(server, addr)
        .await
        .map_err(|e| Error::from(e.to_string()))?;
    eprintln!("serving {SERVER_ID} on http://{bound}/");
    handle.await.map_err(|e| Error::from(e.to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let config_path = std::env::var("APRENDER_SMOKE_CONFIG")
        .map_err(|_| Error::from("APRENDER_SMOKE_CONFIG must name the config.toml to embed"))?;
    let text: &'static str = Box::leak(
        std::fs::read_to_string(&config_path)
            .map_err(|e| Error::from(format!("{config_path}: {e}")))?
            .into_boxed_str(),
    );
    let args: Vec<String> = std::env::args().collect();
    aprender_mcp_builtin::main_with(args, text, SERVER_ID, |server| {
        Box::pin(serve_local(server))
    })
    .await
}
