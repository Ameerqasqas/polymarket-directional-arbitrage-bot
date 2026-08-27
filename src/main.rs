//! CLI entrypoint.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use directional_arbitrage_bot::config::BotConfig;
use directional_arbitrage_bot::ui::{self, LogBuffer};
use directional_arbitrage_bot::workflow::{self, Engine};
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
    /// Run continuously: sample books, size off 60s TWAP, slice clips across the window.
    #[arg(long)]
    daemon: bool,
    /// Disable the live TUI (plain panels + tracing on stdout).
    #[arg(long)]
    plain: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = BotConfig::load(&cli.config)?;

    if cli.daemon && ui::stdout_is_tty() && !cli.plain {
        let logs = LogBuffer::new();
        tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_env_filter(EnvFilter::from_default_env())
            .init();
        return run_tui(cfg, cli.dry_run, logs).await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    if cli.daemon {
        let mut engine = Engine::new(cfg, cli.dry_run, true);
        loop {
            let state = engine.tick().await?;
            ui::print_panel(&state);
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = tokio::time::sleep(engine.poll_interval()) => {}
            }
        }
        return Ok(());
    }

    let state = workflow::run_cycle(&cfg, cli.dry_run).await?;
    ui::print_panel(&state);
    Ok(())
}

async fn run_tui(cfg: BotConfig, dry_run: bool, logs: LogBuffer) -> Result<()> {
    let mut engine = Engine::new(cfg, dry_run, true);
    let interval = engine.poll_interval();
    let mut terminal = ui::enter_tui()?;
    let result = async {
        loop {
            let state = engine.tick().await?;
            ui::draw_dashboard(&mut terminal, &state, &logs)?;
            if ui::quit_pressed()? {
                return Ok(());
            }
            if ui::wait_or_quit(interval).await? {
                return Ok(());
            }
        }
    }
    .await;
    ui::restore_tui(&mut terminal);
    result
}
