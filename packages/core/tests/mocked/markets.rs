//! What is listed and how the protocol stands, read through the client: the trading state, the
//! protocol's figures, the latest trades and a wallet's place in the queue. The reads made on
//! HyperEVM are in [`hyperevm`], and how the relayer's refusals reach a caller in [`relayer`].

mod hyperevm;
mod relayer;

use df_hyperliquid_core::utils::{Address, Dec, hex};
use df_papertrade_core::Error;
use df_papertrade_core::client::deployment::{GENESIS_MS, LISTINGS};
use df_papertrade_core::markets::stats::TradeKind;
use df_papertrade_core::utils::units;

use crate::support::client;
use crate::support::venue::Venue;

/// The wallet whose close was recorded queued 37th.
const QUEUED: &str = "0x9587c978341b4de61fa14e4705f65fbaaec72741";

fn dec(text: &str) -> Dec {
    Dec::parse(text).unwrap()
}

#[tokio::test]
async fn the_trading_state_and_the_protocol_summary_are_read_from_the_relayer() {
    let venue = Venue::new().get("/state/trading", "trading.json").get("/query/protocol/summary", "summary.json");
    let client = client(&venue);

    let state = client.trading_state().await.unwrap();
    let summary = client.summary().await.unwrap();

    assert!(!state.instruments.is_empty());
    assert_eq!(venue.relayed(), ["GET /state/trading", "GET /query/protocol/summary"]);
    for listing in LISTINGS {
        let instrument = state.by_symbol(listing.symbol).unwrap();
        let listed = (instrument.id, instrument.asset_index, instrument.price_scale);
        assert_eq!(listed, (listing.id, listing.asset_index, listing.price_scale), "{}", listing.symbol);
        assert!(instrument.openable(), "{}", listing.symbol);
    }
    assert!(!state.trading_paused && state.accepts("openPosition"));
    assert_eq!(units::usd(state.minimum_margin), Some(Dec::from_int(10)));
    assert_eq!((summary.block, summary.genesis_ms, summary.open_positions), (52_129_296, GENESIS_MS, 667));
    assert_eq!(units::usd(summary.lp), Some(dec("4826009.593836340051193668")));
    assert_eq!(summary.paper.tracked_lp, 4_826_009_593_836_340_051_193_668, "the curve tracks the LP's cash");
    assert_eq!(units::to_dec(summary.paper.mint_rate(), 18), Some(dec("10.474659164463158")), "PAPER a dollar of loss");
}

#[tokio::test]
async fn volumes_the_payout_queue_and_the_latest_trades_are_read_from_the_relayer() {
    let venue = Venue::new()
        .get("/query/protocol/market-stats", "market_stats.json")
        .get("/query/protocol/queue-summary", "queue_summary.json")
        .get("/query/protocol/trades/recent", "trades_recent.json");
    let client = client(&venue);

    let volumes = client.volumes().await.unwrap();
    let queue = client.queue().await.unwrap();
    let trades = client.recent_trades().await.unwrap();

    let btc = units::usd(volumes[0].1).unwrap();
    assert_eq!((volumes[0].0, btc), (0, dec("72621829814.921472515799306976")), "a day of BTC");
    assert_eq!(volumes.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [0, 1]);
    assert_eq!((queue.block, queue.total, queue.count, queue.largest), (48_399_239, 0, 0, None));
    assert_eq!(trades.len(), 50);
    let newest = &trades[0];
    assert_eq!(
        (newest.seq, newest.kind, newest.is_long, newest.instrument_id),
        (48_399_240_000_025, TradeKind::Close, false, 0)
    );
    assert_eq!((newest.exit, newest.pnl_usd), (Some(dec("83335.5")), Some(dec("-327.15"))));
    assert_eq!(newest.wallet.to_hex(), "0xf522d8f0ea3c5d0350d3a05db1df074ecf0b2fd7");
    assert!(trades.windows(2).all(|pair| pair[0].seq > pair[1].seq), "newest first");
    for kind in [TradeKind::Open, TradeKind::Close, TradeKind::Liquidation] {
        assert!(trades.iter().any(|trade| trade.kind == kind), "{kind:?}");
    }
    assert_eq!(
        venue.relayed(),
        ["GET /query/protocol/market-stats", "GET /query/protocol/queue-summary", "GET /query/protocol/trades/recent"]
    );
}

#[tokio::test]
async fn a_wallet_s_place_in_the_queue_is_read_at_its_own_address() {
    let venue = Venue::new().get(&format!("/queue-rank/accounts/{QUEUED}"), "queue_rank.json");
    let checksummed: Address = "0x9587C978341B4DE61FA14E4705F65FBAAEC72741".parse().unwrap();

    let rank = client(&venue).queue_rank(checksummed).await.unwrap();

    assert_eq!(rank.revision, Some(5_646));
    let [(intent_id, position)] = rank.entries.as_slice() else { panic!("{rank:?}") };
    assert_eq!(hex::encode_prefixed(intent_id), "0x668ecd9e1c7dd4045f6332b05d1ec04a57379b17e7ff65a57d88039f793dbd7d");
    assert_eq!(*position, 37);
    assert_eq!(venue.relayed(), [format!("GET /queue-rank/accounts/{QUEUED}")], "lowercase, as the relayer keys it");
}

#[tokio::test]
async fn a_read_whose_answer_does_not_read_names_the_field() {
    let state = Venue::new().get_text("/state/trading", r#"{"schemaVersion":2}"#);
    let summary = Venue::new().get_text("/query/protocol/summary", r#"{"ok":true,"source":{"block":"1"}}"#);
    let trades = Venue::new().get_text("/query/protocol/trades/recent", r#"{"ok":true,"events":[{"kind":"swap"}]}"#);

    let state = client(&state).trading_state().await;
    let summary = client(&summary).summary().await;
    let trades = client(&trades).recent_trades().await.map(|_| ());

    assert_eq!(state.map(|_| ()), Err(Error::Decode("trading_schema: 2".to_owned())));
    assert_eq!(summary.map(|_| ()), Err(Error::Decode("balances: missing".to_owned())));
    assert_eq!(trades, Err(Error::Decode(r#"kind: "swap""#.to_owned())));
}
