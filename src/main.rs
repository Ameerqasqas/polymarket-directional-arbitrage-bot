//! CLI entrypoint.

use std::{path::PathBuf, time::Duration};
use anyhow::Result;
use clap::Parser;
use directional_arbitrage_bot::config::BotConfig;
use directional_arbitrage_bot::workflow;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "directional-arbitrage-bot")]
#[command(about = "Polymarket directional arbitrage (paired arb shell + modeled tilt). Limit orders only.")]
struct Cli {
    /// Path to bot.toml configuration.
    #[arg(short, long, default_value = "bot.toml")]
    config: PathBuf,
    /// Fetch quotes and log the plan without signing or submitting orders.
    #[arg(long)]
    dry_run: bool,
    /// Run continuously, sleeping `polling.interval_ms` between cycles (Ctrl+C to exit).
    #[arg(long)]
    daemon: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let cfg = BotConfig::load(&cli.config)?;

    loop {
        workflow::run_cycle(&cfg, cli.dry_run).await?;

        if !cli.daemon {
            break;
        }

        tokio::time::sleep(Duration::from_millis(cfg.polling.interval_ms)).await;
    }

    Ok(())
}
