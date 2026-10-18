//! The account stream on mainnet, read only: the stream of the busiest wallet in the venue's recent
//! trades, and of the recorded busy wallet, opened at the client's stream URL and read for its
//! first events through the crate's parser. The liveness event must name the heartbeat interval,
//! the first `wallet` frame must fold as a snapshot and any patch within the wait on top of it, and
//! each event's data must have the shape of the recorded streams' (`live_stream.txt`,
//! `live_queued_close.txt`), so a field the relayer has added shows up here first.
//!
//! The client's wire reads whole answers and a stream never ends, so the stream is read through
//! `curl`, a plain `GET`, and dropped at its first heartbeat after a snapshot.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use df_hyperliquid_core::utils::Address;
use df_papertrade_core::account::{Account, Applied};
use df_papertrade_core::live::{self, Event, Events, HEARTBEAT, LIVENESS, WALLET};
use serde_json::Value;

use crate::shape::added;
use crate::support::fixture;
use crate::tap::reader;

/// The longest a stream may take to send its liveness, a snapshot and a heartbeat, in seconds.
const WAIT_SECS: &str = "30";

/// The owner of the recorded `live_stream.txt`.
const RECORDED: &str = "0xa9f28db0543b215eb2705740f10258c3d000cefd";

/// The wallet with the most trades among the venue's latest.
async fn busiest() -> Address {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for trade in reader().recent_trades().await.unwrap() {
        *counts.entry(trade.wallet.to_hex()).or_default() += 1;
    }
    let (wallet, _) = counts.into_iter().max_by_key(|(_, count)| *count).expect("no recent trades");
    wallet.parse().unwrap()
}

/// The stream at `url` up to its first heartbeat after a `wallet` frame, or all that came within
/// [`WAIT_SECS`]: its events as the crate's parser split them, and its text as it came.
fn first_events(url: &str) -> (Vec<Event>, String) {
    let mut curl = Command::new("curl")
        .args(["--silent", "--no-buffer", "--max-time", WAIT_SECS, "--header", "Accept: text/event-stream", url])
        .stdout(Stdio::piped())
        .spawn()
        .expect("curl reads the stream");
    let mut stdout = curl.stdout.take().unwrap();
    let (mut parser, mut events, mut text, mut bytes) = (Events::new(), Vec::new(), String::new(), Vec::new());
    let mut buffer = [0u8; 4096];
    loop {
        let read = stdout.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        let whole = std::str::from_utf8(&bytes).map_or_else(|error| error.valid_up_to(), str::len);
        let chunk = String::from_utf8(bytes.drain(..whole).collect()).unwrap();
        text.push_str(&chunk);
        events.extend(parser.push(&chunk));
        let seen = |name: &str| events.iter().any(|event: &Event| event.name == name);
        if seen(WALLET) && seen(HEARTBEAT) {
            break;
        }
    }
    let _ = curl.kill();
    curl.wait().unwrap();
    (events, text)
}

/// The data of every `name` event in the recorded streams.
fn recorded(name: &str) -> Vec<Value> {
    let events = ["live_stream.txt", "live_queued_close.txt"].map(|capture| Events::new().push(&fixture(capture)));
    let data = events.into_iter().flatten().filter(|event| event.name == name);
    data.map(|event| serde_json::from_str(&event.data).unwrap()).collect()
}

/// The key paths `live` has that none of `recordings` has.
fn new_to_all(recordings: &[Value], live: &Value) -> Vec<String> {
    let mut new = added(&recordings[0], live);
    for recording in &recordings[1..] {
        let also = added(recording, live);
        new.retain(|path| also.contains(path));
    }
    new
}

/// Keeps the stream's text as `target/tmp/live/stream_<wallet>.txt`, to record from.
fn keep(wallet: Address, text: &str) {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("live").join(format!("stream_{}.txt", wallet.to_hex()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
#[ignore = "mainnet"]
async fn a_busy_wallets_stream_folds_and_keeps_its_recorded_shape() {
    let (snapshots, patches): (Vec<Value>, Vec<Value>) =
        recorded(WALLET).into_iter().partition(|frame| frame.get("baseAccountRevision").is_none());
    let liveness = recorded(LIVENESS);
    for wallet in [busiest().await, RECORDED.parse().unwrap()] {
        let (events, text) = first_events(&reader().stream_url(wallet));
        keep(wallet, &text);

        assert_eq!(events.first().map(|event| event.name.as_str()), Some(LIVENESS), "{wallet:?}: {text}");
        assert!(live::heartbeat_ms(&events[0]).is_some(), "{wallet:?}: {}", events[0].data);
        let first: Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(new_to_all(&liveness, &first), Vec::<String>::new(), "{wallet:?}: the liveness event has grown");
        for event in &events[1..] {
            assert!(
                [WALLET, HEARTBEAT].contains(&event.name.as_str()),
                "{wallet:?}: an event it did not send: {event:?}"
            );
            assert!(event.name != HEARTBEAT || event.data == "null", "{wallet:?}: {event:?}");
        }

        let mut held: Option<Account> = None;
        for (index, frame) in events.iter().filter(|event| event.name == WALLET).enumerate() {
            let folded = Account::apply(held.as_ref(), &frame.data, wallet);
            let (account, applied) = folded.unwrap_or_else(|error| panic!("{wallet:?}: {error}: {}", frame.data));
            assert_eq!(applied, if index == 0 { Applied::Snapshot } else { Applied::Patch }, "{wallet:?}");
            let data: Value = serde_json::from_str(&frame.data).unwrap();
            let new = new_to_all(if index == 0 { &snapshots } else { &patches }, &data);
            assert!(new.is_empty(), "{wallet:?}: the relayer's wallet frames have fields they did not have: {new:#?}");
            held = Some(account);
        }
        let account = held.unwrap_or_else(|| panic!("{wallet:?}: no snapshot within {WAIT_SECS} seconds: {text}"));
        assert_eq!(account.address, wallet);
        assert_eq!(account.locked, account.positions.iter().map(|position| position.margin).sum::<u128>());
    }
}
