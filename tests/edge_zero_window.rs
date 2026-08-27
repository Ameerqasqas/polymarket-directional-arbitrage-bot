use directional_arbitrage_bot::config::TwapConfig;
use std::time::Duration;

#[test]
fn zero_window_secs_clamps_to_one_second() {
    let cfg = TwapConfig {
        window_secs: 0,
        slices: 6,
        cooldown_secs: 0,
    };
    assert_eq!(cfg.window(), Duration::from_secs(1));
}
