//! The latest 50 trades as the relayer answered them: opens, closes and liquidations. Each row's
//! kind and side, and a row that does not read.

use super::*;

/// One recorded close with its kind, side and intent id replaced.
fn one(kind: &str, side: &str, intent_id: &str) -> String {
    format!(
        r#"{{"ok":true,"count":1,"events":[{{"seq":"48399240000025","ts":1791862483000,"wallet":"0xf522d8f0ea3c5d0350d3a05db1df074ecf0b2fd7","instrument":"BTC","instrumentId":0,"instrumentTerminal":false,"side":"{side}","leverage":148,"notionalUsd":59200,"intentId":{intent_id},"positionId":"408035","kind":"{kind}","entry":null,"exit":83335.5,"pnlUsd":-327.15}}]}}"#
    )
}

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
    assert!(trades.iter().any(|trade| trade.kind == TradeKind::Liquidation));
    // Prices are read exactly, so every BTC price fits its scale of 10.
    for trade in trades.iter().filter(|trade| trade.instrument == "BTC") {
        for price in [trade.entry, trade.exit].into_iter().flatten() {
            assert!(crate::utils::units::price_raw(price, 10).is_some(), "{price}");
        }
    }
}

#[test]
fn a_row_reads_its_kind_side_and_intent() {
    let id = "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d";
    for (kind, side, intent_id, read) in [
        ("open", "long", "null", Ok((TradeKind::Open, true, false))),
        ("close", "short", "null", Ok((TradeKind::Close, false, false))),
        ("liq", "long", "null", Ok((TradeKind::Liquidation, true, false))),
        ("liquidation", "short", &format!("\"{id}\""), Ok((TradeKind::Liquidation, false, true))),
        ("swap", "long", "null", Err("kind")),
        ("open", "flat", "null", Err("side")),
        ("open", "long", "\"0x66\"", Err("intentId")),
    ] {
        let parsed = recent_trades(&one(kind, side, intent_id)).map(|trades| {
            let trade = &trades[0];
            (trade.kind, trade.is_long, trade.intent_id.is_some())
        });
        assert_eq!(parsed.map_err(|bad| bad.what), read.map_err(str::to_owned), "{kind} {side} {intent_id}");
    }
    let trade = recent_trades(&one("close", "short", "null")).unwrap().remove(0);
    assert_eq!(trade.exit.map(|exit| exit.to_string()), Some("83335.5".to_owned()));
    assert_eq!(trade.pnl_usd.map(|pnl| pnl.to_string()), Some("-327.15".to_owned()));
    assert_eq!((trade.seq, trade.position_id, trade.leverage), (48_399_240_000_025, 408_035, 148));
}
