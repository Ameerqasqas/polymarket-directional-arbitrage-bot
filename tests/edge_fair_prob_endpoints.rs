use directional_arbitrage_bot::strategy::{plan_sizes, StrategyParams};
use rust_decimal_macros::dec;

fn params() -> StrategyParams {
    StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(100),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(0),
        tilt_edge_gap: dec!(1),
    }
}

#[test]
fn fair_probability_zero_and_one_are_allowed() {
    assert!(plan_sizes(dec!(0.48), dec!(0.49), dec!(0), &params(), dec!(1)).is_some());
    assert!(plan_sizes(dec!(0.48), dec!(0.49), dec!(1), &params(), dec!(1)).is_some());
    assert!(plan_sizes(dec!(0.48), dec!(0.49), dec!(0.5), &params(), dec!(1)).is_some());
}
