//! The latest 50 trades as the relayer answered them: opens, closes and liquidations.

use super::*;

#[test]
fn the_recent_trades_read_exactly() {
    let trades =
        recent_trades(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/trades_recent.json"))).unwrap();
    assert_eq!(trades.len(), 50);
    assert!(trades.windows(2).all(|pair| pair[0].seq > pair[1].seq), "newest first");
    let open = trades.iter().find(|trade| trade.kind == TradeKind::Open).unwrap();
    assert!(open.entry.is_some() && open.exit.is_none() && open.pnl_usd.is_none());
    let close = trades.iter().find(|trade| trade.kind != TradeKind::Open).unwrap();
    assert!(close.exit.is_some() && close.pnl_usd.is_some());
    // Prices are read exactly, so every BTC price fits its scale of 10.
    for trade in trades.iter().filter(|trade| trade.instrument == "BTC") {
        for price in [trade.entry, trade.exit].into_iter().flatten() {
            assert!(crate::utils::units::price_raw(price, 10).is_some(), "{price}");
        }
    }
}
