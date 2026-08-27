use directional_arbitrage_bot::market_data::passive_buy_limit;
use rust_decimal_macros::dec;

#[test]
fn passive_buy_limit_clamps_to_one_tick_when_improve_overshoots() {
    let px = passive_buy_limit(dec!(0.50), dec!(0.01), 100).unwrap();
    assert_eq!(px, dec!(0.01));
    assert!(px < dec!(0.50));
}
