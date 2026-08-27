use directional_arbitrage_bot::strategy::{plan_sizes, FavoredOutcome, StrategyParams};
use rust_decimal_macros::dec;

#[test]
fn dust_tilt_extra_is_dropped_to_keep_the_pair() {
    let params = StrategyParams {
        min_locked_edge: dec!(0.02),
        max_usdc_notional: dec!(100),
        base_pair_shares: dec!(50),
        max_tilt_extra_shares: dec!(0.50),
        tilt_edge_gap: dec!(0.02),
    };
    let plan = plan_sizes(dec!(0.45), dec!(0.49), dec!(0.62), &params, dec!(1)).unwrap();
    assert_eq!(plan.favored, FavoredOutcome::A);
    assert_eq!(plan.qty_a, plan.qty_b);
    assert_eq!(plan.qty_a, plan.paired_shares);
    assert!(plan.paired_shares >= dec!(1));
}
