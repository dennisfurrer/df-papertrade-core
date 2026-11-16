//! The account stream as an app reads it: opened at the client's stream URL, its server-sent events
//! split by the crate's parser in whatever chunks arrive, and each `wallet` frame folded into the
//! [`Account`]. The fake relayer serves the captures: a busy wallet over five minutes
//! (`live_stream.txt`) and a wallet with a queued close (`live_queued_close.txt`).

use df_hyperliquid_core::utils::{Address, hex};
use df_papertrade_core::Client;
use df_papertrade_core::account::session_key::{self, Status};
use df_papertrade_core::account::{Account, AccountError, Applied, OperationKind, Outcome};
use df_papertrade_core::client::Call;
use df_papertrade_core::live::{self, Events, HEARTBEAT, LIVENESS, WALLET};

use df_papertrade_core::trade::{Rates, bust_price, value};
use df_papertrade_core::utils::units::usd;

use crate::support::venue::Venue;
use crate::support::{NOW, client};

/// The busy wallet of `live_stream.txt`.
const BUSY: &str = "0xa9f28db0543b215eb2705740f10258c3d000cefd";
/// The wallet of `live_queued_close.txt`.
const QUEUED: &str = "0x9587c978341b4de61fa14e4705f65fbaaec72741";

/// What one reading of a stream gave: the heartbeat interval, the heartbeats, and the account
/// after each `wallet` frame with what the frame did.
struct Read {
    heartbeat_ms: Option<u64>,
    heartbeats: usize,
    frames: Vec<(Applied, Account)>,
}

/// A venue serving `capture` as `wallet`'s stream.
fn serving(wallet: &str, capture: &str) -> Venue {
    Venue::new().get(&format!("/state/user/live?wallet={wallet}"), capture)
}

/// The stream's events, read through the relayer and split in chunks of 97 bytes.
async fn events(client: &Client<Venue>, wallet: Address) -> Vec<live::Event> {
    let text = client.relayer().send(&Call::account_stream(wallet)).await.unwrap();
    let mut events = Events::new();
    text.as_bytes().chunks(97).flat_map(|chunk| events.push(std::str::from_utf8(chunk).unwrap())).collect()
}

/// Folds every `wallet` frame of the stream from no account, as an app does.
async fn read(client: &Client<Venue>, wallet: Address) -> Result<Read, AccountError> {
    let mut read = Read { heartbeat_ms: None, heartbeats: 0, frames: Vec::new() };
    for event in events(client, wallet).await {
        match event.name.as_str() {
            LIVENESS => read.heartbeat_ms = live::heartbeat_ms(&event),
            HEARTBEAT => read.heartbeats += 1,
            WALLET => {
                let held = read.frames.last().map(|(_, account)| account);
                let (account, applied) = Account::apply(held, &event.data, wallet)?;
                read.frames.push((applied, account));
            }
            other => panic!("an event the stream does not send: {other}"),
        }
    }
    Ok(read)
}

fn dollars(raw: u128) -> String {
    usd(raw).unwrap().to_string()
}

#[tokio::test]
async fn a_busy_wallets_stream_folds_into_what_an_app_shows() {
    let wallet: Address = BUSY.parse().unwrap();
    let venue = serving(BUSY, "live_stream.txt").get("/state/trading", "trading.json");
    let client = client(&venue);
    assert_eq!(client.stream_url(wallet), format!("https://exchange.papertrade.xyz/state/user/live?wallet={BUSY}"));

    let read = read(&client, wallet).await.unwrap();

    assert_eq!((read.heartbeat_ms, read.heartbeats, read.frames.len()), (Some(10_000), 12, 13));
    let applied: Vec<Applied> = read.frames.iter().map(|(applied, _)| *applied).collect();
    assert_eq!(applied[0], Applied::Snapshot);
    assert!(applied[1..].iter().all(|applied| *applied == Applied::Patch));
    let first = &read.frames[0].1;
    let ids = |account: &Account| account.positions.iter().map(|position| position.position_id).collect::<Vec<_>>();
    assert_eq!(ids(first), [411_276, 411_085]);
    assert_eq!(dollars(first.funds.available_balance), "234.249163328500998503");

    // Each queued intent shows as an operation, then leaves with its outcome in the next frame.
    for pair in read.frames.windows(2) {
        let (before, after) = (&pair[0].1, &pair[1].1);
        for (intent, outcome) in &after.outcomes {
            assert_eq!(*outcome, Outcome::Succeeded);
            assert!(
                before.operations.iter().any(|operation| operation.intent_id == *intent),
                "{}",
                hex::encode_prefixed(intent)
            );
            assert!(after.operations.iter().all(|operation| operation.intent_id != *intent));
        }
    }
    let queued_opens = read.frames.iter().flat_map(|(_, account)| &account.operations);
    let reserved =
        queued_opens.filter(|operation| matches!(operation.kind, OperationKind::OpenPosition { .. })).count();
    assert_eq!(reserved, 3, "three opens queued");
    let closed: Vec<(u128, i128)> = read
        .frames
        .iter()
        .flat_map(|(_, account)| &account.closed)
        .map(|closed| (closed.position_id, closed.net_pnl))
        .collect();
    assert_eq!(
        closed,
        [(411_276, -108_141_232_449_579_150), (411_379, 113_465_076_220_539_488), (411_413, -372_607_349_980_468_163)]
    );

    let last = &read.frames[12].1;
    assert_eq!((last.account_revision, ids(last)), (13, vec![411_641, 411_085]));
    assert_eq!((dollars(last.balance), dollars(last.locked)), ("233.881879822291490678".to_owned(), "20".to_owned()));
    assert_eq!(last.funds.available_balance, last.balance, "nothing reserved");
    assert_eq!(last.lifetime.realized_pnl, 44_441_814_822_291_490_678, "$44.44 realized since the first deposit");
    assert!(last.operations.is_empty() && last.has_deposited());
    let accumulator = client.trading_state().await.unwrap().acc_reward_per_share;
    assert_eq!(dollars(last.paper.pending_reward(accumulator)), "0.53930777934408627", "claimable staking rewards");
    assert_eq!(venue.relayed(), [format!("GET /state/user/live?wallet={BUSY}"), "GET /state/trading".to_owned()]);
}

#[tokio::test]
async fn a_queued_close_shows_against_its_position_until_it_settles() {
    let wallet: Address = QUEUED.parse().unwrap();
    let read = read(&client(&serving(QUEUED, "live_queued_close.txt")), wallet).await.unwrap();

    assert_eq!((read.heartbeats, read.frames.len()), (4, 3));
    let closing = |account: &Account| -> Vec<(String, Vec<u128>)> {
        let closes = account.operations.iter().filter_map(|operation| match &operation.kind {
            OperationKind::ClosePositions { position_ids } => {
                Some((hex::encode_prefixed(&operation.intent_id)[..10].to_owned(), position_ids.clone()))
            }
            _ => None,
        });
        closes.collect()
    };
    let [(_, snapshot), (_, expired), (_, queued)] = &read.frames[..] else { panic!("three frames") };
    assert_eq!((snapshot.positions.len(), closing(snapshot)), (65, vec![("0x8025d405".to_owned(), vec![407_570])]));
    assert_eq!(expired.outcomes.iter().map(|(_, outcome)| *outcome).collect::<Vec<_>>(), [Outcome::Expired]);
    assert!(closing(expired).is_empty(), "the expired close is gone and the position stays");
    assert_eq!(closing(queued), [("0x668ecd9e".to_owned(), vec![407_570])], "closed again, queued");
    let operation = &queued.operations[0];
    assert_eq!((operation.accepted_at_ms, operation.deadline), (1_791_862_481_214, 1_791_863_080));
    assert!(queued.positions.iter().any(|position| position.position_id == 407_570), "open until the close executes");

    let (key, expiry) = queued.session_keys[0];
    assert_eq!((queued.session_keys.len(), expiry), (1, 1_794_321_534));
    let held = (expiry + 1) * 1000;
    let status = session_key::status(key, held, &queued.session_keys, queued.indexed_through_timestamp, NOW);
    assert_eq!(status, Status::Ready { expires_at: expiry }, "its session key signs for another month");
}

#[tokio::test]
async fn a_stream_that_misses_a_frame_is_read_again_from_its_snapshot() {
    let wallet: Address = BUSY.parse().unwrap();
    let venue = serving(BUSY, "live_stream.txt");
    let client = client(&venue);
    let frames: Vec<String> = events(&client, wallet)
        .await
        .into_iter()
        .filter(|event| event.name == WALLET)
        .map(|event| event.data)
        .collect();

    let mut held = Account::apply(None, &frames[0], wallet).unwrap().0;
    held = Account::apply(Some(&held), &frames[1], wallet).unwrap().0;
    let gap = Account::apply(Some(&held), &frames[3], wallet);
    assert_eq!(gap, Err(AccountError::OutOfSync("the patch is for another revision")), "the third frame never came");

    let again = read(&client, wallet).await.unwrap();
    assert_eq!(again.frames.last().unwrap().1.account_revision, 13);
    assert_eq!(venue.relayed().len(), 2, "the stream was opened twice");
}

#[tokio::test]
async fn an_app_values_each_streamed_position_at_the_mark() {
    let wallet: Address = BUSY.parse().unwrap();
    let venue = serving(BUSY, "live_stream.txt").get("/state/trading", "trading.json");
    let client = client(&venue);
    let read = read(&client, wallet).await.unwrap();
    let state = client.trading_state().await.unwrap();

    let account = &read.frames[12].1;
    for position in &account.positions {
        let instrument = state.instrument(position.instrument_id).unwrap();
        let bust = bust_price(position.entry_price, position.leverage, position.is_long, instrument.bust_buffer);
        assert_eq!(bust, Some(position.bust_price), "{}: the venue's bust price is ours", position.position_id);
        let at = |mark| value(position, instrument, mark, Rates::default()).unwrap();
        let (flat, busted) = (at(position.entry_price), at(position.bust_price));
        assert_eq!((flat.raw_pnl, flat.payout_pnl, flat.risk_bp, flat.liquidated), (0, 0, Some(0), false));
        assert_eq!((busted.risk_bp, busted.liquidated), (Some(10_000), true));
        // the move at bust is a buffer short of the margin, and the close settles as a liquidation
        let moved = busted.raw_pnl.unsigned_abs();
        assert!(busted.raw_pnl < 0 && moved < position.margin, "bust a buffer before the margin is gone: {moved}");
        assert_eq!(busted.payout_pnl.unsigned_abs(), position.margin, "the whole margin");
        let gain = if position.is_long { position.entry_price * 101 / 100 } else { position.entry_price * 99 / 100 };
        let won = at(gain);
        assert!(0 < won.payout_pnl && won.payout_pnl < won.adjusted_pnl && won.adjusted_pnl < won.raw_pnl, "{won:?}");
    }
    let open: Vec<(u32, bool)> =
        account.positions.iter().map(|position| (position.instrument_id, position.is_long)).collect();
    assert_eq!(open, [(0, false), (1, false)], "a BTC short and an ETH short");
}
