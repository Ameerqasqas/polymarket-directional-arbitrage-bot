use directional_arbitrage_bot::twap::{QuoteSample, QuoteTwap};
use rust_decimal_macros::dec;
use std::time::{Duration, Instant};

#[test]
fn simultaneous_samples_use_the_last_print() {
    let t0 = Instant::now();
    let mut tw = QuoteTwap::new(Duration::from_secs(60));
    tw.push(QuoteSample {
        at: t0,
        ask_a: dec!(0.10),
        ask_b: dec!(0.90),
    });
    tw.push(QuoteSample {
        at: t0,
        ask_a: dec!(0.40),
        ask_b: dec!(0.50),
    });
    let q = tw.value(t0 + Duration::from_millis(1)).unwrap();
    assert_eq!(q.ask_a, dec!(0.40));
    assert_eq!(q.ask_b, dec!(0.50));
}
