use directional_arbitrage_bot::strategy::{plan_sizes, FavoredOutcome, StrategyParams};
use rust_decimal_macros::dec;

fn params() -> StrategyParams {
    StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(100),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(25),
        tilt_edge_gap: dec!(0.02),
    }
}

#[test]
fn tilts_to_b_when_model_prefers_b() {
    let plan = plan_sizes(dec!(0.49), dec!(0.45), dec!(0.38), &params(), dec!(1)).unwrap();
    assert_eq!(plan.favored, FavoredOutcome::B);
    assert!(plan.qty_b > plan.qty_a);
    assert_eq!(plan.qty_a, plan.paired_shares);
    assert!(plan.qty_b > plan.paired_shares);
}
