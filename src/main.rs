use std::net::SocketAddr;
use std::process::ExitCode;

use anyhow::Context;
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

mod assets;
mod browser;
mod config;
mod pty;
mod server;
mod terminal;

use crate::config::Cli;

fn init_tracing(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(&cli.log_level);

    if let Err(err) = run(&cli).await {
        warn!(error = ?err, "browsterm exited with error");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

async fn run(cli: &Cli) -> anyhow::Result<()> {
    // Resolve bind address.
    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port)
        .parse()
        .with_context(|| format!("invalid --host/--port: {}:{}", cli.host, cli.port))?;

    // Build the server state from the (already resolved) shell + args.
    let (shell, args) = cli.resolved_shell();
    let state = server::ServerState::new(shell, args);
    let listener = state.bind(addr).await.context("bind failed")?;
    let local_addr = listener
        .local_addr()
        .context("could not determine the bound port")?;

    info!(addr = %local_addr, "browsterm listening");

    // Auto-open the browser unless suppressed.
    let url = format!("http://{}/", local_addr);
    if !cli.no_browser {
        if let Err(err) = browser::open_url(&url).await {
            warn!(error = ?err, "could not auto-open browser; visit {} manually", url);
        } else {
            info!(url = %url, "opened browser to {url}");
        }
    } else {
        info!(url = %url, "--no-browser set; visit {url} manually");
    }

    // Serve until Ctrl-C.
    let shutdown = async {
        if tokio::signal::ctrl_c().await.is_ok() {
            info!("received Ctrl-C, shutting down");
        }
    };

    server::serve(listener, state, shutdown).await
}
