//! Local runner for the thin forecast MCP server.
//!
//! Default transport is stdio — what MCP clients (Claude Desktop, Claude Code, Cursor)
//! spawn directly. `--http [PORT]` serves the same one tool over streamable-HTTP plus the
//! same-origin demo page; `--bench [sizes...]` prints a fit/predict table on synthetic
//! series. Everything human-readable goes to stderr: stdout belongs to the protocol. The
//! `--bench` table is a report on a non-protocol run, so it prints to stdout by design.
//!
//! ```bash
//! aprender-mcp-forecast                 # stdio
//! aprender-mcp-forecast --http 8765     # demo page + /mcp on loopback
//! aprender-mcp-forecast --bench 1000    # timing table
//! ```

use std::process::ExitCode;
use std::time::Instant;

use aprender_forecast::prophet::{days_from_civil, format_ymd, Rng};
use aprender_forecast::ForecastArgs;

/// Server name advertised in `initialize`.
const SERVER_NAME: &str = "aprender-forecast";
/// Default `--http` port.
const DEFAULT_PORT: u16 = 8765;

/// A synthetic daily series with drifting slope, yearly + weekly terms and noise.
fn synth(n: usize, seed: u64) -> (Vec<String>, Vec<f64>) {
    let mut rng = Rng::new(seed);
    let start = days_from_civil(2010, 1, 1);
    let mut slope = 0.002;
    let mut ds = Vec::with_capacity(n);
    let mut y = Vec::with_capacity(n);
    let mut level = 10.0;
    for i in 0..n {
        let day = start + i as i64;
        if i % 400 == 0 && i > 0 {
            slope += (rng.uniform() - 0.5) * 0.004;
        }
        level += slope;
        let f = day as f64;
        let v = level
            + 0.8 * (2.0 * std::f64::consts::PI * f / 365.25).sin()
            + 0.3 * (2.0 * std::f64::consts::PI * f / 7.0).cos()
            + 0.2 * rng.normal();
        ds.push(format_ymd(day));
        y.push(v);
    }
    (ds, y)
}

/// The `--bench` report. Not a protocol path: these `println!`s are the report itself.
fn bench(sizes: &[usize]) {
    println!("| points | model | fit s | predict s | total s | L-BFGS rounds/iters/evals |");
    println!("|---|---|---|---|---|---|");
    for &n in sizes {
        let (ds, y) = synth(n, 3);
        let args = ForecastArgs {
            ds,
            y,
            horizon: 365,
            freq: None,
            model: Some("prophet".into()),
            growth: None,
            cap: None,
            seasonality_mode: None,
            interval_width: None,
            holidays: None,
            n_lags: None,
            seed: None,
        };
        let t0 = Instant::now();
        match aprender_forecast::forecast(&args) {
            Ok(r) => {
                let total = t0.elapsed().as_secs_f64();
                let lb = &r.diagnostics["lbfgs"];
                println!(
                    "| {n} | prophet | {:.3} | {:.3} | {total:.3} | {}/{}/{} |",
                    r.fit_seconds,
                    r.predict_seconds,
                    lb["rounds"],
                    lb["iterations"],
                    lb["evaluations"]
                );
            }
            Err(e) => eprintln!("error: {n} points refused: {e}"),
        }
    }
}

fn build() -> Option<pmcp::Server> {
    match aprender_mcp_forecast::build_server(SERVER_NAME, env!("CARGO_PKG_VERSION")) {
        Ok(server) => Some(server),
        Err(error) => {
            eprintln!("error: server construction refused: {error}");
            None
        }
    }
}

async fn serve_http(port: u16) -> ExitCode {
    let Some(server) = build() else {
        return ExitCode::FAILURE;
    };
    let app = aprender_mcp_forecast::http_app(server);
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("error: bind {port}: {error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "{SERVER_NAME}: demo page http://127.0.0.1:{port}/ — MCP streamable-http at \
         http://127.0.0.1:{port}/mcp"
    );
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn serve_stdio() -> ExitCode {
    let Some(server) = build() else {
        return ExitCode::FAILURE;
    };
    eprintln!(
        "{SERVER_NAME}: serving `{}` on stdio",
        aprender_mcp_forecast::TOOL_NAME
    );
    if let Err(error) = server.run_stdio().await {
        eprintln!("error: stdio server terminated: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--bench") => {
            let sizes: Vec<usize> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
            bench(if sizes.is_empty() {
                &[1_000, 3_000, 10_000, 20_000]
            } else {
                &sizes
            });
            ExitCode::SUCCESS
        }
        Some("--http") => {
            let port: u16 = args
                .get(1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT);
            serve_http(port).await
        }
        Some(other) if other.starts_with('-') => {
            eprintln!(
                "error: unknown argument {other}; usage: aprender-mcp-forecast \
                 [--http PORT] [--bench SIZES...]"
            );
            ExitCode::from(2)
        }
        _ => serve_stdio().await,
    }
}
