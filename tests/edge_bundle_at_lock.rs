use directional_arbitrage_bot::strategy::{plan_sizes, StrategyParams};
use rust_decimal_macros::dec;

#[test]
fn bundle_exactly_at_locked_edge_is_tradable() {
    let params = StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(100),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(0),
        tilt_edge_gap: dec!(1),
    };
    let plan = plan_sizes(dec!(0.49), dec!(0.49), dec!(0.50), &params, dec!(1));
    assert!(plan.is_some(), "ask_a + ask_b == 1 - min_locked_edge must still quote");
}
