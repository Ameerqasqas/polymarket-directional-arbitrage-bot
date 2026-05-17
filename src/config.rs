//! Bot configuration loaded from TOML.

use std::path::Path;

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::Deserialize;

/// Wallet signing mode for Polymarket CLOB authentication.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind {
    #[default]
    Eoa,
    /// Browser-wallet style proxy (Magic/email flows often use this path).
    Proxy,
    /// Gnosis Safe-style Polymarket wallet derived from the signing EOA.
    GnosisSafe,
}

#[derive(Debug, Deserialize, Default)]
pub struct WalletConfig {
    #[serde(default)]
    pub signature_type: SignatureKind,
}

#[derive(Debug, Deserialize)]
pub struct TokenRef {
    /// Human-readable label for logs only (e.g. `Up`, `Down`).
    pub label: String,
    /// Outcome token id as decimal digits (Gamma / UI token id).
    pub token_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MarketConfig {
    pub outcome_a: TokenRef,
    pub outcome_b: TokenRef,
}

#[derive(Debug, Deserialize)]
pub struct StrategyConfig {
    /// Model-implied probability that outcome **A** resolves YES (0–1).
    pub fair_probability_a: Decimal,
    /// Require `ask_a + ask_b <= 1 - min_locked_edge` before quoting.
    pub min_locked_edge: Decimal,
    /// Cap total USDC spent this cycle (`qty_a * price_a + qty_b * price_b`).
    pub max_usdc_notional: Decimal,
    /// Maximum paired hedge size (the arb “shell”) on **each** outcome before tilt.
    pub base_pair_shares: Decimal,
    /// Maximum extra shares on the favored outcome beyond the paired hedge.
    pub max_tilt_extra_shares: Decimal,
    /// Minimum `(edge_favored - edge_other)` required to apply tilt.
    pub tilt_edge_gap: Decimal,
    /// Improve maker price by N ticks below best ask on each BUY limit (0 = join ask).
    pub price_improve_ticks: u32,
}

#[derive(Debug, Deserialize)]
pub struct PollingConfig {
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_interval_ms(),
        }
    }
}

fn default_interval_ms() -> u64 {
    1500
}

#[derive(Debug, Deserialize)]
pub struct BotConfig {
    /// `https://clob.polymarket.com` (USDC.e / legacy) or `https://clob-v2.polymarket.com` (pUSD / V2).
    #[serde(default = "default_clob_host")]
    pub clob_host: String,
    #[serde(default)]
    pub wallet: WalletConfig,
    pub market: MarketConfig,
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub polling: PollingConfig,
}

fn default_clob_host() -> String {
    "https://clob.polymarket.com".to_owned()
}

impl BotConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read config file {}",
                path.display()
            )
        })?;
        toml::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
    }
}
