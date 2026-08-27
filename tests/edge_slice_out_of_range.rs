use directional_arbitrage_bot::twap::slice_quantity;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn slice_index_past_the_end_is_zero() {
    assert_eq!(slice_quantity(dec!(10), 5, 3), Decimal::ZERO);
    assert_eq!(slice_quantity(dec!(10), 99, 6), Decimal::ZERO);
}
