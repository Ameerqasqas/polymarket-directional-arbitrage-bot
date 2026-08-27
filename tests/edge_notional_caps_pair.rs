use directional_arbitrage_bot::strategy::{plan_sizes, StrategyParams};
use rust_decimal_macros::dec;

#[test]
fn usdc_notional_cap_shrinks_paired_size_below_base() {
    let params = StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(10),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(0),
        tilt_edge_gap: dec!(0.02),
    };
    let plan = plan_sizes(dec!(0.48), dec!(0.49), dec!(0.50), &params, dec!(1)).unwrap();
    assert!(plan.paired_shares < params.base_pair_shares);
    assert!(plan.paired_shares * (dec!(0.48) + dec!(0.49)) <= params.max_usdc_notional);
    assert_eq!(plan.qty_a, plan.qty_b);
}
