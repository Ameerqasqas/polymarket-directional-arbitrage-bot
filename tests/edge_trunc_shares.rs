use directional_arbitrage_bot::strategy::trunc_shares;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn trunc_shares_clamps_negative_to_zero() {
    assert_eq!(trunc_shares(dec!(-1.25)), Decimal::ZERO);
    assert_eq!(trunc_shares(dec!(1.239)), dec!(1.23));
    assert_eq!(trunc_shares(Decimal::ZERO), Decimal::ZERO);
}
