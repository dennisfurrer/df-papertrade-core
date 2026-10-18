//! An empty payout queue as the relayer answered it.

use super::*;

#[test]
fn an_empty_queue_reads() {
    let queue =
        QueueSummary::parse(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/queue_summary.json"))).unwrap();
    assert_eq!((queue.total, queue.count, queue.front), (0, 0, None));
}
