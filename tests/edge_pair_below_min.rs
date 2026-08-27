use directional_arbitrage_bot::strategy::{plan_sizes, StrategyParams};
use rust_decimal_macros::dec;

#[test]
fn rejects_when_paired_size_is_below_venue_minimum() {
    let params = StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(5),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(0),
        tilt_edge_gap: dec!(0.02),
    };
    assert!(plan_sizes(dec!(0.48), dec!(0.49), dec!(0.50), &params, dec!(10)).is_none());
}
