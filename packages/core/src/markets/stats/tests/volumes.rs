//! Market volumes as the relayer answered them, keyed by instrument id.

use super::*;

#[test]
fn the_market_volumes_read_by_id() {
    let volumes =
        market_volumes(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/market_stats.json"))).unwrap();
    assert_eq!(volumes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
}
