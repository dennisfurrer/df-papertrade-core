use hl_core::Dec;

use super::*;

#[test]
fn a_scale_is_a_power_of_ten() {
    assert_eq!(scale_places(1), Some(0));
    assert_eq!(scale_places(10), Some(1));
    assert_eq!(scale_places(100), Some(2));
    assert_eq!(scale_places(0), None);
    assert_eq!(scale_places(20), None);
}

#[test]
fn prices_read_at_their_scale() {
    assert_eq!(price(829_105, 10), Some(Dec::parse("82910.5").unwrap()));
    assert_eq!(price(249_235, 100), Some(Dec::parse("2492.35").unwrap()));
    assert_eq!(price_raw(Dec::parse("82910.5").unwrap(), 10), Some(829_105));
    assert_eq!(price_raw(Dec::parse("82910.55").unwrap(), 10), None, "finer than the market quotes");
}

/// The oracle's `(bid + ask) / 2`, toward zero, never overflowing.
#[test]
fn the_mid_is_the_oracles() {
    assert_eq!(mid(829_100, 829_110), Some(829_105));
    assert_eq!(mid(829_100, 829_101), Some(829_100));
    assert_eq!(mid(u128::MAX, u128::MAX), Some(u128::MAX));
    assert_eq!(mid(0, 1), None);
    assert_eq!(mid(2, 1), None, "crossed");
}

#[test]
fn collateral_is_eighteen_decimals() {
    assert_eq!(usd(10_000_000_000_000_000_000), Some(Dec::from_int(10)));
    assert_eq!(collateral(Dec::parse("10.5").unwrap()), Some(10_500_000_000_000_000_000));
    assert_eq!(collateral(Dec::parse("-1").unwrap()), None);
    assert_eq!(usdc_to_collateral(10_000_000), 10_000_000_000_000_000_000);
    assert_eq!(collateral_to_usdc(10_999_999_999_999_999_999), Some(10_999_999));
}
