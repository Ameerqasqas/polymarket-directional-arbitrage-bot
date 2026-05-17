//! Order book helpers (best ask, passive BUY limit derivation).

use anyhow::Result;
use polymarket_client_sdk::clob::types::response::OrderBookSummaryResponse;
use polymarket_client_sdk::types::Decimal;

/// Best price to lift asks immediately (smallest ask quote).
pub fn best_ask(book: &OrderBookSummaryResponse) -> Option<Decimal> {
    book.asks
        .iter()
        .filter(|lvl| lvl.size > Decimal::ZERO)
        .map(|lvl| lvl.price)
        .min()
}

/// BUY limit price snapped to tick, optionally stepping inside the spread by whole ticks.
pub fn passive_buy_limit(best_ask: Decimal, tick: Decimal, improve_ticks: u32) -> Result<Decimal> {
    if tick <= Decimal::ZERO {
        anyhow::bail!("tick must be positive");
    }
    let decimals = tick.scale();
    let step = tick * Decimal::from(improve_ticks);
    let raw = best_ask - step;
    let clamped = raw.max(tick);
    Ok(clamped.trunc_with_scale(decimals))
}
