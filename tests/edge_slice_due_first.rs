use directional_arbitrage_bot::twap::slice_due_at;
use std::time::Duration;

#[test]
fn first_twap_clip_is_due_immediately() {
    assert_eq!(slice_due_at(Duration::from_secs(60), 0, 6), Duration::ZERO);
    assert_eq!(
        slice_due_at(Duration::from_secs(60), 3, 6),
        Duration::from_secs(30)
    );
    assert_eq!(slice_due_at(Duration::from_secs(60), 1, 6), Duration::from_secs(10));
}
