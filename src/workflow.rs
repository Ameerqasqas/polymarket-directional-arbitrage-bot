//! End-to-end cycle: sample books → 60s TWAP → size plan → sliced limits.

use std::str::FromStr as _;
use std::time::Instant;

use alloy::signers::Signer as _;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob::types::SignatureType;
use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;
use polymarket_client_sdk::clob::{Client, Config};
use polymarket_client_sdk::types::{Decimal, U256};
use polymarket_client_sdk::{POLYGON, PRIVATE_KEY_VAR};
use tracing::{info, warn};

use crate::config::{BotConfig, SignatureKind};
use crate::execution;
use crate::market_data::{best_ask, passive_buy_limit};
use crate::strategy::{plan_sizes, SizePlan, StrategyParams};
use crate::twap::{leftover_clips_expired, paired_slice, slice_due_at, QuoteSample, QuoteTwap};
use crate::ui::DashboardState;

struct LiveTrading {
    client: Client<Authenticated<Normal>>,
    signer: PrivateKeySigner,
}

struct TwapRun {
    started: Instant,
    plan: SizePlan,
    next_slice: u32,
    slices: u32,
    /// In-flight clip after one leg filled and the other failed.
    open_clip: Option<ClipFill>,
}

#[derive(Debug, Clone)]
struct ClipFill {
    id_a: Option<String>,
    id_b: Option<String>,
    qty_a: Decimal,
    qty_b: Decimal,
}

impl ClipFill {
    fn complete(&self) -> bool {
        (self.qty_a <= Decimal::ZERO || self.id_a.is_some())
            && (self.qty_b <= Decimal::ZERO || self.id_b.is_some())
    }

    fn needs_a(&self) -> bool {
        self.qty_a > Decimal::ZERO && self.id_a.is_none()
    }

    fn needs_b(&self) -> bool {
        self.qty_b > Decimal::ZERO && self.id_b.is_none()
    }
}

pub struct Engine {
    cfg: BotConfig,
    dry_run: bool,
    tolerate_errors: bool,
    oneshot: bool,
    twap: QuoteTwap,
    exec: Option<TwapRun>,
    trading: Option<LiveTrading>,
    submitted_a: Decimal,
    submitted_b: Decimal,
    last_order_a: Option<String>,
    last_order_b: Option<String>,
    cooldown_until: Option<Instant>,
    last_error: Option<String>,
}

pub struct QuoteSnapshot {
    pub ask_a: Decimal,
    pub ask_b: Decimal,
    pub tick_a: Decimal,
    pub tick_b: Decimal,
    pub min_order_size: Decimal,
    pub token_a: U256,
    pub token_b: U256,
}

impl Engine {
    pub fn new(cfg: BotConfig, dry_run: bool, tolerate_errors: bool) -> Self {
        let window = cfg.twap.window();
        Self {
            cfg,
            dry_run,
            tolerate_errors,
            oneshot: false,
            twap: QuoteTwap::new(window),
            exec: None,
            trading: None,
            submitted_a: Decimal::ZERO,
            submitted_b: Decimal::ZERO,
            last_order_a: None,
            last_order_b: None,
            cooldown_until: None,
            last_error: None,
        }
    }

    pub fn poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.cfg.polling.interval_ms.max(200))
    }

    pub async fn tick(&mut self) -> Result<DashboardState> {
        match self.tick_inner().await {
            Ok(state) => {
                self.last_error = None;
                Ok(state)
            }
            Err(err) if self.tolerate_errors => {
                let msg = format!("{err:#}");
                warn!(error = %msg, "cycle failed; will retry");
                self.last_error = Some(msg);
                Ok(self.view(None, None, "retrying after error"))
            }
            Err(err) => Err(err),
        }
    }

    async fn tick_inner(&mut self) -> Result<DashboardState> {
        let live = fetch_snapshot(&self.cfg).await?;
        let now = Instant::now();
        self.twap.push(QuoteSample {
            at: now,
            ask_a: live.ask_a,
            ask_b: live.ask_b,
        });
        let tw = self.twap.value(now);
        let ready = self.is_twap_ready(now);

        let (size_ask_a, size_ask_b) = match tw {
            Some(q) => (q.ask_a, q.ask_b),
            None => (live.ask_a, live.ask_b),
        };

        let strat = StrategyParams {
            min_locked_edge: self.cfg.strategy.min_locked_edge,
            max_usdc_notional: self.cfg.strategy.max_usdc_notional,
            base_pair_shares: self.cfg.strategy.base_pair_shares,
            max_tilt_extra_shares: self.cfg.strategy.max_tilt_extra_shares,
            tilt_edge_gap: self.cfg.strategy.tilt_edge_gap,
        };

        let plan = plan_sizes(
            size_ask_a,
            size_ask_b,
            self.cfg.strategy.fair_probability_a,
            &strat,
            live.min_order_size,
        );

        let improve = self.cfg.strategy.price_improve_ticks;
        let px_a = passive_buy_limit(live.ask_a, live.tick_a, improve)?;
        let px_b = passive_buy_limit(live.ask_b, live.tick_b, improve)?;

        if let Some(p) = &plan {
            info!(
                favored = ?p.favored,
                paired_shares = %p.paired_shares,
                qty_a = %p.qty_a,
                qty_b = %p.qty_b,
                twap_a = %size_ask_a,
                twap_b = %size_ask_b,
                live_a = %live.ask_a,
                live_b = %live.ask_b,
                ready,
                "TWAP directional plan"
            );
        } else {
            warn!(
                twap_a = %size_ask_a,
                twap_b = %size_ask_b,
                bundle = %(size_ask_a + size_ask_b),
                ready,
                "No actionable TWAP plan (bundle too loose or sizes below minimum)"
            );
        }

        let status = self
            .advance_execution(now, ready, plan.as_ref(), &live, px_a, px_b)
            .await?;

        Ok(self.view(Some(&live), plan.as_ref(), &status))
    }

    async fn advance_execution(
        &mut self,
        now: Instant,
        ready: bool,
        plan: Option<&SizePlan>,
        live: &QuoteSnapshot,
        px_a: Decimal,
        px_b: Decimal,
    ) -> Result<String> {
        if self.exec.is_some() {
            let (elapsed, next_slice, slices, has_open) = {
                let run = self.exec.as_ref().expect("TWAP run");
                (
                    now.saturating_duration_since(run.started),
                    run.next_slice,
                    run.slices,
                    run.open_clip.is_some(),
                )
            };
            if leftover_clips_expired(elapsed, self.cfg.twap.window()) && !has_open {
                self.abort_run(
                    "TWAP window elapsed; skipping leftover clips instead of bursting",
                );
                return Ok("TWAP window elapsed — leftover clips skipped".into());
            }
            if plan.is_none() {
                self.abort_run("arb window closed; aborting remaining TWAP clips");
                return Ok("TWAP aborted — bundle no longer locked".into());
            }
            let due = slice_due_at(self.cfg.twap.window(), next_slice, slices);
            if elapsed >= due {
                return self.fire_next_clip(live, px_a, px_b).await;
            }
            return Ok(format!(
                "TWAP in progress — next clip {}/{}",
                next_slice + 1,
                slices
            ));
        }

        if !ready {
            return Ok(format!(
                "warming TWAP ({:.0}s / {}s)",
                self.twap
                    .value(now)
                    .map(|q| q.span.as_secs_f64())
                    .unwrap_or(0.0),
                self.cfg.twap.window_secs
            ));
        }

        if let Some(until) = self.cooldown_until {
            if now < until {
                return Ok("TWAP cooldown".into());
            }
        }

        let Some(plan) = plan.cloned() else {
            return Ok("watching — no locked edge on 60s TWAP".into());
        };

        self.exec = Some(TwapRun {
            started: now,
            plan,
            next_slice: 0,
            slices: self.cfg.twap.slice_count(),
            open_clip: None,
        });
        self.submitted_a = Decimal::ZERO;
        self.submitted_b = Decimal::ZERO;
        self.fire_next_clip(live, px_a, px_b).await
    }

    fn abort_run(&mut self, reason: &str) {
        warn!("{reason}");
        self.exec = None;
        self.cooldown_until = Some(Instant::now() + self.cfg.twap.cooldown());
    }

    async fn fire_next_clip(
        &mut self,
        live: &QuoteSnapshot,
        px_a: Decimal,
        px_b: Decimal,
    ) -> Result<String> {
        let (slice_idx, slices, mut fill) = {
            let run = self.exec.as_mut().expect("TWAP run");
            if let Some(open) = run.open_clip.take() {
                (run.next_slice, run.slices, open)
            } else {
                let (qty_a, qty_b) = paired_slice(
                    run.plan.qty_a,
                    run.plan.qty_b,
                    run.next_slice,
                    run.slices,
                    live.min_order_size,
                );
                (
                    run.next_slice,
                    run.slices,
                    ClipFill {
                        id_a: None,
                        id_b: None,
                        qty_a,
                        qty_b,
                    },
                )
            }
        };

        if self.dry_run {
            info!(
                slice = slice_idx + 1,
                slices,
                clip_a = %fill.qty_a,
                clip_b = %fill.qty_b,
                px_a = %px_a,
                px_b = %px_b,
                "dry-run: skip signing and submission"
            );
            fill.id_a = (fill.qty_a > Decimal::ZERO).then(|| "dry-run-a".into());
            fill.id_b = (fill.qty_b > Decimal::ZERO).then(|| "dry-run-b".into());
        } else if fill.qty_a > Decimal::ZERO || fill.qty_b > Decimal::ZERO {
            if let Err(err) = self.place_missing(live, px_a, px_b, &mut fill).await {
                if let Some(run) = self.exec.as_mut() {
                    run.open_clip = Some(fill);
                }
                warn!(error = %err, "clip hedge incomplete; will retry only the missing leg");
                if self.tolerate_errors {
                    return Ok(
                        "hedge incomplete — retrying the missing leg (will not re-post the filled side)"
                            .into(),
                    );
                }
                return Err(err).context("submit TWAP clip");
            }
        }

        if fill.id_a.as_deref().is_some_and(|id| id != "dry-run-a") {
            self.last_order_a = fill.id_a.clone();
        }
        if fill.id_b.as_deref().is_some_and(|id| id != "dry-run-b") {
            self.last_order_b = fill.id_b.clone();
        }

        self.submitted_a += fill.qty_a;
        self.submitted_b += fill.qty_b;

        let finished = {
            let run = self.exec.as_mut().expect("TWAP run");
            run.next_slice += 1;
            run.next_slice >= run.slices
        };

        if finished {
            self.exec = None;
            self.cooldown_until = Some(Instant::now() + self.cfg.twap.cooldown());
            return Ok(format!("TWAP window complete ({slices} clips)"));
        }

        Ok(format!(
            "submitted TWAP clip {}/{slices}",
            slice_idx + 1
        ))
    }

    async fn place_missing(
        &mut self,
        live: &QuoteSnapshot,
        px_a: Decimal,
        px_b: Decimal,
        fill: &mut ClipFill,
    ) -> Result<()> {
        if self.trading.is_none() {
            self.trading = Some(connect_trading(&self.cfg).await?);
        }
        let t = self.trading.as_ref().expect("trading client");
        if fill.needs_a() {
            let id = execution::place_buy_gtc_limit(
                &t.client,
                &t.signer,
                live.token_a,
                px_a,
                fill.qty_a,
            )
            .await
            .context("submit BUY outcome A")?;
            fill.id_a = Some(id);
        }
        if fill.needs_b() {
            let id = execution::place_buy_gtc_limit(
                &t.client,
                &t.signer,
                live.token_b,
                px_b,
                fill.qty_b,
            )
            .await
            .context("submit BUY outcome B after A filled")?;
            fill.id_b = Some(id);
        }
        info!(
            order_a = fill.id_a.as_deref().unwrap_or(""),
            order_b = fill.id_b.as_deref().unwrap_or(""),
            "Submitted TWAP BUY limits"
        );
        if !fill.complete() {
            anyhow::bail!("clip still missing a hedge leg after submit");
        }
        Ok(())
    }

    fn is_twap_ready(&self, now: Instant) -> bool {
        if self.oneshot {
            return self.twap.sample_count() >= 1;
        }
        self.twap
            .value(now)
            .map(|q| q.span >= self.cfg.twap.window())
            .unwrap_or(false)
    }

    fn view(
        &self,
        live: Option<&QuoteSnapshot>,
        plan: Option<&SizePlan>,
        status: &str,
    ) -> DashboardState {
        let now = Instant::now();
        let tw = self.twap.value(now);
        let improve = self.cfg.strategy.price_improve_ticks;
        let (px_a, px_b) = if let Some(live) = live {
            (
                passive_buy_limit(live.ask_a, live.tick_a, improve).ok(),
                passive_buy_limit(live.ask_b, live.tick_b, improve).ok(),
            )
        } else {
            (None, None)
        };
        let ready = self.is_twap_ready(now);
        DashboardState {
            dry_run: self.dry_run,
            label_a: self.cfg.market.outcome_a.label.clone(),
            label_b: self.cfg.market.outcome_b.label.clone(),
            fair_a: self.cfg.strategy.fair_probability_a,
            min_locked_edge: self.cfg.strategy.min_locked_edge,
            live_ask_a: live.map(|l| l.ask_a),
            live_ask_b: live.map(|l| l.ask_b),
            twap_ask_a: tw.map(|q| q.ask_a),
            twap_ask_b: tw.map(|q| q.ask_b),
            twap_span_secs: tw.map(|q| q.span.as_secs_f64()).unwrap_or(0.0),
            twap_window_secs: self.cfg.twap.window_secs,
            twap_samples: self.twap.sample_count(),
            twap_ready: ready,
            plan: plan.cloned(),
            px_a,
            px_b,
            slice_idx: self.exec.as_ref().map(|r| r.next_slice).unwrap_or(0),
            slices: self.cfg.twap.slice_count(),
            executing: self.exec.is_some(),
            submitted_a: self.submitted_a,
            submitted_b: self.submitted_b,
            last_order_a: self.last_order_a.clone(),
            last_order_b: self.last_order_b.clone(),
            status: status.to_string(),
            error: self.last_error.clone(),
        }
    }
}

async fn fetch_snapshot(cfg: &BotConfig) -> Result<QuoteSnapshot> {
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

    Ok(QuoteSnapshot {
        ask_a,
        ask_b,
        tick_a: book_a.tick_size.as_decimal(),
        tick_b: book_b.tick_size.as_decimal(),
        min_order_size: book_a.min_order_size.max(book_b.min_order_size),
        token_a,
        token_b,
    })
}

async fn connect_trading(cfg: &BotConfig) -> Result<LiveTrading> {
    let pk = std::env::var(PRIVATE_KEY_VAR).context(format!(
        "missing {} for live execution",
        PRIVATE_KEY_VAR
    ))?;
    let signer = PrivateKeySigner::from_str(&pk)
        .context("parse POLYMARKET_PRIVATE_KEY")?
        .with_chain_id(Some(POLYGON));

    let config = Config::builder().use_server_time(true).build();
    let client = Client::new(&cfg.clob_host, config)?;

    let client = match cfg.wallet.signature_type {
        SignatureKind::Eoa => client
            .authentication_builder(&signer)
            .authenticate()
            .await
            .context("CLOB authenticate (EOA)")?,
        SignatureKind::Proxy => client
            .authentication_builder(&signer)
            .signature_type(SignatureType::Proxy)
            .authenticate()
            .await
            .context("CLOB authenticate (Proxy)")?,
        SignatureKind::GnosisSafe => client
            .authentication_builder(&signer)
            .signature_type(SignatureType::GnosisSafe)
            .authenticate()
            .await
            .context("CLOB authenticate (GnosisSafe)")?,
    };

    Ok(LiveTrading { client, signer })
}

/// Single evaluation + optional execution against current books.
///
/// One-shot mode uses the live snapshot as a 1-sample TWAP and submits the
/// full plan as a single clip. Daemon mode should use [`Engine`] instead so
/// clips are spread across the 60s window.
pub async fn run_cycle(cfg: &BotConfig, dry_run: bool) -> Result<DashboardState> {
    let mut engine = Engine::new(cfg.clone(), dry_run, false);
    engine.oneshot = true;
    engine.cfg.twap.slices = 1;
    engine.tick().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn retries_only_the_unfilled_hedge_leg() {
        let fill = ClipFill {
            id_a: Some("0xabc".into()),
            id_b: None,
            qty_a: dec!(10),
            qty_b: dec!(10),
        };
        assert!(!fill.needs_a());
        assert!(fill.needs_b());
        assert!(!fill.complete());
    }

    #[test]
    fn zero_qty_leg_does_not_need_an_order_id() {
        let fill = ClipFill {
            id_a: Some("0xabc".into()),
            id_b: None,
            qty_a: dec!(10),
            qty_b: Decimal::ZERO,
        };
        assert!(fill.complete());
        assert!(!fill.needs_b());
    }
}
