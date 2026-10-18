//! The summary as the relayer answered it on 2026-10-13.

use super::*;

#[test]
fn the_summary_reads() {
    let summary = Summary::parse(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/summary.json"))).unwrap();
    assert!(summary.tvl > summary.lp && summary.open_positions > 0);
    assert_eq!(summary.queue, 0);
    assert_eq!(summary.paper.cliff, 2_000_000 * 10u128.pow(18));
    assert_eq!(summary.paper.cap, 5_000_000 * 10u128.pow(18));
    assert_eq!(summary.paper.rate, 100 * 10u128.pow(18));
    assert!(summary.paper.mint_rate() < summary.paper.rate, "past the cliff the rate has decayed");
}
