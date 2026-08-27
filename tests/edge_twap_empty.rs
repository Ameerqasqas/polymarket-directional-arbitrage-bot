use directional_arbitrage_bot::twap::QuoteTwap;
use std::time::{Duration, Instant};

#[test]
fn empty_twap_window_has_no_value() {
    let tw = QuoteTwap::new(Duration::from_secs(60));
    assert!(tw.value(Instant::now()).is_none());
    assert_eq!(tw.sample_count(), 0);
}
