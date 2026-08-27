use directional_arbitrage_bot::config::TwapConfig;

#[test]
fn zero_slice_count_clamps_to_one_clip() {
    let cfg = TwapConfig {
        window_secs: 60,
        slices: 0,
        cooldown_secs: 0,
    };
    assert_eq!(cfg.slice_count(), 1);
    assert_eq!(cfg.window().as_secs(), 60);
}
