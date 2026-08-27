//! 60-second TWAP of complementary asks, plus clip sizing for sliced execution.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use rust_decimal::Decimal;

use crate::strategy::trunc_shares;

#[derive(Debug, Clone, Copy)]
pub struct QuoteSample {
    pub at: Instant,
    pub ask_a: Decimal,
    pub ask_b: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwapQuote {
    pub ask_a: Decimal,
    pub ask_b: Decimal,
    pub sample_count: usize,
    pub span: Duration,
}

impl TwapQuote {
    pub fn bundle(self) -> Decimal {
        self.ask_a + self.ask_b
    }
}

/// Rolling time-weighted average of both outcome asks.
#[derive(Debug, Clone)]
pub struct QuoteTwap {
    window: Duration,
    samples: VecDeque<QuoteSample>,
}

impl QuoteTwap {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::new(),
        }
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    pub fn push(&mut self, sample: QuoteSample) {
        self.samples.push_back(sample);
        self.evict(sample.at);
    }

    fn evict(&mut self, now: Instant) {
        while let Some(front) = self.samples.front() {
            if now.saturating_duration_since(front.at) > self.window {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn value(&self, now: Instant) -> Option<TwapQuote> {
        if self.samples.is_empty() {
            return None;
        }
        let span = now.saturating_duration_since(self.samples.front()?.at);
        let ask_a = time_weighted(self.samples.iter().map(|s| (s.at, s.ask_a)), now)?;
        let ask_b = time_weighted(self.samples.iter().map(|s| (s.at, s.ask_b)), now)?;
        Some(TwapQuote {
            ask_a,
            ask_b,
            sample_count: self.samples.len(),
            span,
        })
    }
}

fn time_weighted(
    samples: impl IntoIterator<Item = (Instant, Decimal)>,
    now: Instant,
) -> Option<Decimal> {
    let pts: Vec<(Instant, Decimal)> = samples.into_iter().collect();
    if pts.is_empty() {
        return None;
    }
    if pts.len() == 1 {
        return Some(pts[0].1);
    }

    let mut num = Decimal::ZERO;
    let mut den = Decimal::ZERO;
    for i in 0..pts.len() {
        let t0 = pts[i].0;
        let t1 = if i + 1 < pts.len() { pts[i + 1].0 } else { now };
        let dt_ms = t1.saturating_duration_since(t0).as_millis() as u64;
        if dt_ms == 0 {
            continue;
        }
        let w = Decimal::from(dt_ms);
        num += pts[i].1 * w;
        den += w;
    }
    if den == Decimal::ZERO {
        return pts.last().map(|p| p.1);
    }
    Some(num / den)
}

/// Split `total` into `slices` clips (2-dp shares). Last clip absorbs remainder.
pub fn slice_quantity(total: Decimal, slice_idx: u32, slices: u32) -> Decimal {
    let slices = slices.max(1);
    let total = trunc_shares(total);
    if slices == 1 || slice_idx >= slices {
        return if slice_idx == 0 || slice_idx + 1 == slices {
            total
        } else {
            Decimal::ZERO
        };
    }
    let base = trunc_shares(total / Decimal::from(slices));
    if slice_idx + 1 == slices {
        trunc_shares(total - base * Decimal::from(slices - 1)).max(Decimal::ZERO)
    } else {
        base
    }
}

/// Elapsed TWAP window fraction at which clip `slice_idx` should fire (0-based).
pub fn slice_due_at(window: Duration, slice_idx: u32, slices: u32) -> Duration {
    let slices = slices.max(1);
    if slice_idx == 0 {
        return Duration::ZERO;
    }
    window.mul_f64(f64::from(slice_idx) / f64::from(slices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::time::Duration;

    #[test]
    fn equal_interval_twap_is_mean() {
        let t0 = Instant::now();
        let mut tw = QuoteTwap::new(Duration::from_secs(60));
        tw.push(QuoteSample {
            at: t0,
            ask_a: dec!(0.40),
            ask_b: dec!(0.50),
        });
        tw.push(QuoteSample {
            at: t0 + Duration::from_secs(30),
            ask_a: dec!(0.50),
            ask_b: dec!(0.40),
        });
        let q = tw.value(t0 + Duration::from_secs(60)).unwrap();
        assert_eq!(q.ask_a, dec!(0.45));
        assert_eq!(q.ask_b, dec!(0.45));
        assert_eq!(q.sample_count, 2);
    }

    #[test]
    fn evicts_samples_older_than_window() {
        let t0 = Instant::now();
        let mut tw = QuoteTwap::new(Duration::from_secs(60));
        tw.push(QuoteSample {
            at: t0,
            ask_a: dec!(0.10),
            ask_b: dec!(0.90),
        });
        tw.push(QuoteSample {
            at: t0 + Duration::from_secs(61),
            ask_a: dec!(0.50),
            ask_b: dec!(0.50),
        });
        let q = tw.value(t0 + Duration::from_secs(61)).unwrap();
        assert_eq!(q.ask_a, dec!(0.50));
        assert_eq!(q.sample_count, 1);
    }

    #[test]
    fn last_slice_absorbs_remainder() {
        assert_eq!(slice_quantity(dec!(40), 0, 6), dec!(6.66));
        assert_eq!(slice_quantity(dec!(40), 5, 6), dec!(6.70));
        let sum: Decimal = (0..6).map(|i| slice_quantity(dec!(40), i, 6)).sum();
        assert_eq!(sum, dec!(40));
    }

    #[test]
    fn single_slice_is_full_size() {
        assert_eq!(slice_quantity(dec!(12.34), 0, 1), dec!(12.34));
    }
}
