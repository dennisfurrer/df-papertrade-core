//! The relayer's reads, from mainnet: the trading state, the protocol's figures, the latest trades,
//! a busy wallet's place in the queue and the venue's health, each held against its recording
//! right after it is read.
//! Only `GET`s: a deposit check registers the wallet, so it is never asked.

use df_papertrade_core::client::Call;
use df_papertrade_core::client::deployment::{GENESIS_MS, LISTINGS};
use df_papertrade_core::trade::math::WIN_FEE_RATE;
use df_papertrade_core::utils::units;

use crate::tap::{reader, same_shape};

/// The trading state as the relayer answers it since `intakeControls`, the leverage bounds and
/// `winFeeRateRaw` were added; `trading.json` is the answer from before.
#[tokio::test]
#[ignore = "mainnet"]
async fn the_trading_state_lists_its_markets() {
    let client = reader();
    let state = client.trading_state().await.unwrap();
    assert!(!state.instruments.is_empty() && state.intake.is_some(), "{state:?}");
    same_shape(&client, "trading_mainnet.json");
    for listing in LISTINGS {
        let instrument = state.instrument(listing.id).unwrap();
        let listed = (instrument.symbol.as_str(), instrument.asset_index, instrument.price_scale);
        assert_eq!(listed, (listing.symbol, listing.asset_index, listing.price_scale));
        let (lowest, highest) = state.leverage_range(instrument);
        assert!(1 <= lowest && lowest <= highest, "{}: {lowest} to {highest}", listing.symbol);
    }
    assert_eq!(state.win_fee_rate, Some(WIN_FEE_RATE), "the relayer's win fee is the one the math assumes");
    assert!(state.minimum_margin > 0 && state.minimum_open_notional >= state.minimum_margin);
}

#[tokio::test]
#[ignore = "mainnet"]
async fn the_protocol_summary_reads() {
    let client = reader();
    let summary = client.summary().await.unwrap();
    same_shape(&client, "summary.json");
    assert_eq!(summary.genesis_ms, GENESIS_MS);
    assert!(summary.at_ms > GENESIS_MS && summary.tvl >= summary.lp, "{summary:?}");
    assert!(summary.paper.staked <= summary.paper.supply, "{:?}", summary.paper);
}

#[tokio::test]
#[ignore = "mainnet"]
async fn each_listed_market_has_a_day_s_volume() {
    let client = reader();
    let volumes = client.volumes().await.unwrap();
    same_shape(&client, "market_stats.json");
    let ids: Vec<u32> = volumes.iter().map(|(id, _)| *id).collect();
    assert!(LISTINGS.iter().all(|listing| ids.contains(&listing.id)), "{ids:?}");
}

#[tokio::test]
#[ignore = "mainnet"]
async fn the_payout_queue_reads() {
    let client = reader();
    let queue = client.queue().await.unwrap();
    same_shape(&client, "queue_summary.json");
    assert_eq!(queue.count == 0, queue.total == 0, "{queue:?}");
    assert_eq!(queue.count == 0, queue.front.is_none(), "{queue:?}");
}

#[tokio::test]
#[ignore = "mainnet"]
async fn the_latest_trades_read_newest_first_at_their_market_s_scale() {
    let client = reader();
    let trades = client.recent_trades().await.unwrap();
    same_shape(&client, "trades_recent.json");
    assert!(!trades.is_empty() && trades.len() <= 50, "{}", trades.len());
    assert!(trades.windows(2).all(|pair| pair[0].seq > pair[1].seq), "newest first");
    for trade in &trades {
        let listing = LISTINGS.iter().find(|listing| listing.id == trade.instrument_id).unwrap();
        for price in [trade.entry, trade.exit].into_iter().flatten() {
            assert!(units::price_raw(price, listing.price_scale).is_some(), "{} at {price}", listing.symbol);
        }
    }
}

/// The wallet of the latest trade, so one that trades now.
#[tokio::test]
#[ignore = "mainnet"]
async fn a_busy_wallet_s_place_in_the_queue_reads() {
    let client = reader();
    let busy = client.recent_trades().await.unwrap()[0].wallet;
    let rank = client.queue_rank(busy).await.unwrap();
    same_shape(&client, "queue_rank.json");
    assert!(rank.entries.iter().all(|(_, position)| *position > 0), "{rank:?}");
}

#[tokio::test]
#[ignore = "mainnet"]
async fn the_venue_says_it_is_up_on_hyperevm() {
    let client = reader();
    let health: serde_json::Value =
        serde_json::from_str(&client.relayer().send(&Call::health()).await.unwrap()).unwrap();
    assert_eq!(
        (&health["status"], &health["chainId"]),
        (&serde_json::json!("ok"), &serde_json::json!(999)),
        "{health}"
    );
}
