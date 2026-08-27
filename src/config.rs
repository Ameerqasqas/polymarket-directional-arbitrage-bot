//! Bot configuration loaded from TOML.

use std::path::Path;
use std::time::Duration;

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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WalletConfig {
    #[serde(default)]
    pub signature_type: SignatureKind,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenRef {
    /// Human-readable label for logs only (e.g. `Up`, `Down`).
    pub label: String,
    /// Outcome token id as decimal digits (Gamma / UI token id).
    pub token_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketConfig {
    pub outcome_a: TokenRef,
    pub outcome_b: TokenRef,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TwapConfig {
    /// Rolling window used to average asks before sizing (seconds).
    #[serde(default = "default_twap_window_secs")]
    pub window_secs: u64,
    /// Child clips spread across the same window once TWAP is ready.
    #[serde(default = "default_twap_slices")]
    pub slices: u32,
    /// Wait this long after a TWAP window finishes before starting another.
    #[serde(default)]
    pub cooldown_secs: u64,
}

impl Default for TwapConfig {
    fn default() -> Self {
        Self {
            window_secs: default_twap_window_secs(),
            slices: default_twap_slices(),
            cooldown_secs: 0,
        }
    }
}

fn default_twap_window_secs() -> u64 {
    60
}

fn default_twap_slices() -> u32 {
    6
}

impl TwapConfig {
    pub fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs.max(1))
    }

    pub fn slice_count(&self) -> u32 {
        self.slices.max(1)
    }

    pub fn cooldown(&self) -> Duration {
        Duration::from_secs(self.cooldown_secs)
    }
}

#[derive(Debug, Clone, Deserialize)]
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
    #[serde(default)]
    pub twap: TwapConfig,
}

fn default_clob_host() -> String {
    "https://clob.polymarket.com".to_owned()
}

impl BotConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).with_context(|| {
            format!("failed to read config file {}", path.display())
        })?;
        let cfg: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        let a = self.market.outcome_a.token_id.trim();
        let b = self.market.outcome_b.token_id.trim();
        if a.is_empty() || b.is_empty() {
            anyhow::bail!("outcome token_id must not be empty");
        }
        if a == b {
            anyhow::bail!(
                "outcome_a and outcome_b must be different tokens (both are {a})"
            );
        }
        if !a.chars().all(|c| c.is_ascii_digit()) || !b.chars().all(|c| c.is_ascii_digit()) {
            anyhow::bail!("outcome token_id must be a decimal digit string (not a placeholder)");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_toml(token_a: &str, token_b: &str) -> String {
        format!(
            r#"
clob_host = "https://clob.polymarket.com"
[market.outcome_a]
label = "Up"
token_id = "{token_a}"
[market.outcome_b]
label = "Down"
token_id = "{token_b}"
[strategy]
fair_probability_a = "0.58"
min_locked_edge = "0.015"
max_usdc_notional = "250"
base_pair_shares = "40"
max_tilt_extra_shares = "80"
tilt_edge_gap = "0.02"
price_improve_ticks = 0
"#
        )
    }

    #[test]
    fn rejects_identical_outcome_tokens() {
        let cfg: BotConfig = toml::from_str(&sample_toml("123", "123")).unwrap();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("different tokens"), "{err}");
    }

    #[test]
    fn rejects_placeholder_token_ids() {
        let cfg: BotConfig =
            toml::from_str(&sample_toml("REPLACE_WITH_UP_TOKEN_ID_DECIMAL_STRING", "456")).unwrap();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("placeholder"), "{err}");
    }

    #[test]
    fn rejects_empty_token_ids() {
        let cfg: BotConfig = toml::from_str(&sample_toml("   ", "456")).unwrap();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn accepts_distinct_numeric_tokens() {
        let cfg: BotConfig = toml::from_str(&sample_toml("123", "456")).unwrap();
        cfg.validate().unwrap();
    }
}
