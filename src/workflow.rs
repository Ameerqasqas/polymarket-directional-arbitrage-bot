//! End-to-end cycle: pull books → size plan → quote limits → optionally submit.

use std::str::FromStr as _;

use alloy::signers::local::LocalSigner;
use anyhow::{Context, Result};
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob::types::SignatureType;
use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;
use polymarket_client_sdk::clob::{Client, Config};
use polymarket_client_sdk::types::U256;
use polymarket_client_sdk::{POLYGON, PRIVATE_KEY_VAR};
use tracing::{info, warn};

use crate::config::{BotConfig, SignatureKind};
use crate::execution;
use crate::market_data::{best_ask, passive_buy_limit};
use crate::strategy::{plan_sizes, StrategyParams};

async fn authenticate_client(
    cfg: &BotConfig,
    signer: &LocalSigner,
) -> Result<Client<Authenticated>> {
    let config = Config::builder().use_server_time(true).build();
    let client = Client::new(&cfg.clob_host, config)?;

    let auth = match cfg.wallet.signature_type {
        SignatureKind::Eoa => client
            .authentication_builder(signer)
            .authenticate()
            .await
            .context("CLOB authenticate (EOA)")?,
        SignatureKind::Proxy => client
            .authentication_builder(signer)
            .signature_type(SignatureType::Proxy)
            .authenticate()
            .await
            .context("CLOB authenticate (Proxy)")?,
        SignatureKind::GnosisSafe => client
            .authentication_builder(signer)
            .signature_type(SignatureType::GnosisSafe)
            .authenticate()
            .await
            .context("CLOB authenticate (GnosisSafe)")?,
    };

    Ok(auth)
}

/// Single evaluation + optional execution against current books.
pub async fn run_cycle(cfg: &BotConfig, dry_run: bool) -> Result<()> {
    let book_client = Client::new(
        &cfg.clob_host,
        Config::builder().use_server_time(true).build(),
    )
    .context("construct read-only CLOB client")?;

    let token_a =
        U256::from_str(&cfg.market.outcome_a.token_id).context("parse outcome_a.token_id")?;
    let token_b =
        U256::from_str(&cfg.market.outcome_b.token_id).context("parse outcome_b.token_id")?;

    let req_a = OrderBookSummaryRequest::builder().token_id(token_a).build();
    let req_b = OrderBookSummaryRequest::builder().token_id(token_b).build();

    let book_a = book_client
        .order_book(&req_a)
        .await
        .context("fetch orderbook outcome A")?;
    let book_b = book_client
        .order_book(&req_b)
        .await
        .context("fetch orderbook outcome B")?;

    let ask_a = best_ask(&book_a).context("empty ask side on outcome A")?;
    let ask_b = best_ask(&book_b).context("empty ask side on outcome B")?;

    let tick_a = book_a.tick_size.as_decimal();
    let tick_b = book_b.tick_size.as_decimal();

    let min_sz = book_a.min_order_size.max(book_b.min_order_size);

    let strat = StrategyParams {
        min_locked_edge: cfg.strategy.min_locked_edge,
        max_usdc_notional: cfg.strategy.max_usdc_notional,
        base_pair_shares: cfg.strategy.base_pair_shares,
        max_tilt_extra_shares: cfg.strategy.max_tilt_extra_shares,
        tilt_edge_gap: cfg.strategy.tilt_edge_gap,
    };

    let Some(plan) = plan_sizes(
        ask_a,
        ask_b,
        cfg.strategy.fair_probability_a,
        &strat,
        min_sz,
    ) else {
        warn!(
            ask_a = %ask_a,
            ask_b = %ask_b,
            bundle = %(ask_a + ask_b),
            "No actionable plan (bundle too loose or sizes below minimum)"
        );
        return Ok(());
    };

    let improve = cfg.strategy.price_improve_ticks;
    let px_a = passive_buy_limit(ask_a, tick_a, improve)?;
    let px_b = passive_buy_limit(ask_b, tick_b, improve)?;

    let notion = plan.qty_a * px_a + plan.qty_b * px_b;
    info!(
        favored = ?plan.favored,
        paired_shares = %plan.paired_shares,
        qty_a = %plan.qty_a,
        qty_b = %plan.qty_b,
        px_a = %px_a,
        px_b = %px_b,
        approx_usdc = %notion,
        label_a = %cfg.market.outcome_a.label,
        label_b = %cfg.market.outcome_b.label,
        "Quoted directional arb ladder"
    );

    if dry_run {
        info!("dry-run: skip signing and submission");
        return Ok(());
    }

    let pk = std::env::var(PRIVATE_KEY_VAR).context(format!(
        "missing {} for live execution",
        PRIVATE_KEY_VAR
    ))?;
    let signer = LocalSigner::from_str(&pk)
        .context("parse POLYMARKET_PRIVATE_KEY")?
        .with_chain_id(Some(POLYGON));

    let trading_client = authenticate_client(cfg, &signer).await?;

    let id_a = execution::place_buy_gtc_limit(
        &trading_client,
        &signer,
        token_a,
        px_a,
        plan.qty_a,
    )
    .await
    .context("submit BUY outcome A")?;
    let id_b = execution::place_buy_gtc_limit(
        &trading_client,
        &signer,
        token_b,
        px_b,
        plan.qty_b,
    )
    .await
    .context("submit BUY outcome B")?;

    info!(order_a = %id_a, order_b = %id_b, "Submitted paired BUY limits");

    Ok(())
}
