//! Market volumes as the relayer answered them, keyed by instrument id, and answers that do not
//! read.

use super::*;

#[test]
fn the_market_volumes_read_by_id() {
    let volumes =
        market_volumes(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/market_stats.json"))).unwrap();
    assert_eq!(volumes, [(0, 72_621_829_814_921_472_515_799_306_976), (1, 42_698_366_377_828_069_376_730_120_912)]);
    let unsorted = r#"{"markets":{"1":{"volume_24h":"5"},"0":{"volume_24h":"7"}}}"#;
    assert_eq!(market_volumes(unsorted), Ok(vec![(0, 7), (1, 5)]), "sorted by id");
}

#[test]
fn volumes_that_do_not_read_name_what_failed() {
    for (text, what) in [
        (r#"{"ok":true}"#, "markets"),
        (r#"{"markets":[]}"#, "markets"),
        (r#"{"markets":{"BTC":{"volume_24h":"7"}}}"#, "market_id"),
        (r#"{"markets":{"0":{}}}"#, "volume_24h"),
        (r#"{"markets":{"0":{"volume_24h":7.5}}}"#, "volume_24h"),
        ("", "market_stats"),
    ] {
        assert_eq!(market_volumes(text).map_err(|bad| bad.what), Err(what.to_owned()), "{text}");
    }
}
