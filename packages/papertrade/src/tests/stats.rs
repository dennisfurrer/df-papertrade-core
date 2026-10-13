use hl_core::hex;

use super::*;

#[test]
fn the_summary_reads() {
    let summary = Summary::parse(include_str!("../../fixtures/summary.json")).unwrap();
    assert!(summary.tvl > summary.lp && summary.open_positions > 0);
    assert_eq!(summary.queue, 0);
    assert_eq!(summary.paper.cliff, 2_000_000 * 10u128.pow(18));
    assert_eq!(summary.paper.cap, 5_000_000 * 10u128.pow(18));
    assert_eq!(summary.paper.rate, 100 * 10u128.pow(18));
    assert!(summary.paper.mint_rate() < summary.paper.rate, "past the cliff the rate has decayed");
}

#[test]
fn the_markets_queue_and_trades_read() {
    let volumes = market_volumes(include_str!("../../fixtures/market_stats.json")).unwrap();
    assert_eq!(volumes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
    let queue = QueueSummary::parse(include_str!("../../fixtures/queue_summary.json")).unwrap();
    assert_eq!((queue.total, queue.count, queue.front), (0, 0, None));

    let trades = recent_trades(include_str!("../../fixtures/trades_recent.json")).unwrap();
    assert_eq!(trades.len(), 50);
    assert!(trades.windows(2).all(|pair| pair[0].seq > pair[1].seq), "newest first");
    let open = trades.iter().find(|trade| trade.kind == TradeKind::Open).unwrap();
    assert!(open.entry.is_some() && open.exit.is_none() && open.pnl_usd.is_none());
    let close = trades.iter().find(|trade| trade.kind != TradeKind::Open).unwrap();
    assert!(close.exit.is_some() && close.pnl_usd.is_some());
    // Read as written, every BTC price is exact at its scale of ten.
    for trade in trades.iter().filter(|trade| trade.instrument == "BTC") {
        for price in [trade.entry, trade.exit].into_iter().flatten() {
            assert!(crate::units::price_raw(price, 10).is_some(), "{price}");
        }
    }
}

#[test]
fn a_queue_rank_reads() {
    let rank = QueueRank::parse(include_str!("../../fixtures/queue_rank.json")).unwrap();
    assert_eq!(rank.revision, Some(5_646));
    assert_eq!(rank.entries.len(), 1);
    assert_eq!(
        hex::encode_prefixed(&rank.entries[0].0),
        "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d"
    );
    assert_eq!(rank.entries[0].1, 37);
}

#[test]
fn a_deposit_check_names_the_proxy() {
    let proxy = deposit_address(r#"{"proxy":"0xc125C9A929Bd75555a027DD563C89A116Bd41C50"}"#).unwrap();
    assert_eq!(proxy.to_hex(), "0xc125c9a929bd75555a027dd563c89a116bd41c50");
}
