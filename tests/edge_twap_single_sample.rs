use directional_arbitrage_bot::twap::{QuoteSample, QuoteTwap};
use rust_decimal_macros::dec;
use std::time::{Duration, Instant};

#[test]
fn single_sample_twap_equals_that_print() {
    let t0 = Instant::now();
    let mut tw = QuoteTwap::new(Duration::from_secs(60));
    tw.push(QuoteSample {
        at: t0,
        ask_a: dec!(0.41),
        ask_b: dec!(0.55),
    });
    let q = tw.value(t0 + Duration::from_secs(5)).unwrap();
    assert_eq!(q.ask_a, dec!(0.41));
    assert_eq!(q.ask_b, dec!(0.55));
    assert_eq!(q.sample_count, 1);
}
