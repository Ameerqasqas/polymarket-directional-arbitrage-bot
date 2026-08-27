use directional_arbitrage_bot::market_data::passive_buy_limit;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn passive_buy_limit_rejects_non_positive_tick() {
    assert!(passive_buy_limit(dec!(0.50), Decimal::ZERO, 0).is_err());
    assert!(passive_buy_limit(dec!(0.50), dec!(-0.01), 0).is_err());
}
