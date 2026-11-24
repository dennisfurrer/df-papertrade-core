//! The account's open interest caps and recent nonces: read from the snapshot recorded on
//! 2026-11-24 (`live_recent_nonces.txt`, eleven BTC positions, no caps of its own), and from the
//! same snapshot edited to carry caps, a queued open and patches.

use df_hyperliquid_core::utils::Address;
use serde_json::{Value, json};

use super::*;
use crate::account::state::{Account, AccountError, Applied};
use crate::live::{Events, WALLET};
use crate::test_support::{RECENT, STREAM, TRADING, TRADING_NOW};

const DOLLAR: u128 = 1_000_000_000_000_000_000;

fn frame(stream: &str) -> Value {
    let event = Events::new().push(stream).into_iter().find(|event| event.name == WALLET).unwrap();
    serde_json::from_str(&event.data).unwrap()
}

fn wallet(frame: &Value) -> Address {
    frame["address"].as_str().unwrap().parse().unwrap()
}

fn fold(frame: &Value) -> Result<Account, AccountError> {
    Account::apply(None, &frame.to_string(), wallet(frame)).map(|(account, _)| account)
}

fn recorded() -> Account {
    fold(&frame(RECENT)).unwrap()
}

fn edited(edit: impl Fn(&mut Value)) -> Result<Account, AccountError> {
    let mut frame = frame(RECENT);
    edit(&mut frame);
    fold(&frame)
}

#[test]
fn the_recorded_snapshot_reads_its_recent_nonces_and_no_caps_of_its_own() {
    let account = recorded();
    assert_eq!(account.recent_nonces, RecentNonces { exchange: Some(182_791_869), staking: Some(609_122_819) });
    assert_eq!(account.recent_nonces.get(crate::utils::Lane::Exchange), Some(182_791_869));
    assert!(account.oi_caps.is_empty());
    assert_eq!(account.positions.len(), 11);
}

/// 35 dollars at 870, 444, 444, 149, 149, 131 and four times 238, and 10 dollars at 793:
/// $117,795 of notional on BTC, under the venue's $10M account cap.
#[test]
fn the_venue_s_cap_less_what_the_account_holds_is_its_room() {
    let (account, state) = (recorded(), TradingState::parse(TRADING_NOW).unwrap());
    assert_eq!(state.account_oi_cap, Some(10_000_000 * DOLLAR));
    assert_eq!(account.oi_used(0), 117_795 * DOLLAR);
    assert_eq!(account.oi_used(1), 0);
    assert_eq!(account.oi_cap(&state, 0), Some(10_000_000 * DOLLAR));
    assert_eq!(account.oi_room(&state, 0), Some(9_882_205 * DOLLAR));
    assert_eq!(account.oi_room(&state, 1), Some(10_000_000 * DOLLAR));
    let uncapped = TradingState::parse(TRADING).unwrap();
    assert_eq!((uncapped.account_oi_cap, account.oi_room(&uncapped, 0)), (None, None), "no cap before 2026-11");
}

/// A cap for the market wins over one for every market, which wins over the venue's. A `null`
/// cap is no cap.
#[test]
fn the_account_s_own_caps_override_the_venue_s() {
    let state = TradingState::parse(TRADING_NOW).unwrap();
    let account = edited(|frame| frame["oiCaps"] = json!([[0, "200000000000000000000000"], [null, null]])).unwrap();
    assert_eq!(
        account.oi_caps,
        [OiCap { instrument_id: Some(0), cap: Some(200_000 * DOLLAR) }, OiCap { instrument_id: None, cap: None },]
    );
    assert_eq!(account.oi_room(&state, 0), Some(82_205 * DOLLAR));
    assert_eq!(account.oi_room(&state, 1), None, "every other market uncapped");
    let account = edited(|frame| frame["oiCaps"] = json!([[null, "100000000000000000000000"]])).unwrap();
    assert_eq!(account.oi_room(&state, 0), Some(0), "past the cap is no room, not less");
    assert_eq!(account.oi_room(&state, 1), Some(100_000 * DOLLAR));
}

#[test]
fn a_queued_open_counts_against_the_cap() {
    let open = json!([
        "0xbacd5844863a2af1f983e864300642a0f03949de025cf3208372d670f6f44937",
        "openPosition",
        0,
        false,
        false,
        "10000000000000000000",
        100,
        1795530203000u64,
        "1795530293",
        null
    ]);
    let account = edited(|frame| frame["operations"] = json!([open])).unwrap();
    assert_eq!(account.oi_used(0), 118_795 * DOLLAR);
    assert_eq!(account.oi_used(1), 0);
}

/// A patch with `oiCaps` replaces them; one without keeps them. The recent nonces are the
/// snapshot's.
#[test]
fn a_patch_replaces_the_caps_it_carries() {
    let snapshot = edited(|frame| frame["oiCaps"] = json!([[1, "5"]])).unwrap();
    let wallet = snapshot.address;
    let patch = |revision: u64, caps: Option<Value>| {
        let mut patch = json!({
            "address": wallet.to_hex(), "accountRevision": revision, "baseAccountRevision": revision - 1,
            "addedPositions": [], "removedPositionIds": [], "addedOperations": [], "removedOperationIds": [],
        });
        if let Some(caps) = caps {
            patch["oiCaps"] = caps;
        }
        patch.to_string()
    };
    let (kept, applied) = Account::apply(Some(&snapshot), &patch(309, None), wallet).unwrap();
    assert_eq!((applied, kept.oi_caps.clone()), (Applied::Patch, snapshot.oi_caps.clone()));
    let (cleared, _) = Account::apply(Some(&kept), &patch(310, Some(json!([]))), wallet).unwrap();
    assert!(cleared.oi_caps.is_empty());
    assert_eq!(cleared.recent_nonces, snapshot.recent_nonces);
}

/// A snapshot from before the relayer sent either field reads as no caps and no nonces seen.
#[test]
fn an_older_snapshot_has_neither() {
    let account = fold(&frame(STREAM)).unwrap();
    assert!(account.oi_caps.is_empty());
    assert_eq!(account.recent_nonces, RecentNonces::default());
    let partial = edited(|frame| frame["recentNonces"] = json!({"session": null})).unwrap();
    assert_eq!(partial.recent_nonces, RecentNonces::default());
}

#[test]
fn caps_and_nonces_that_do_not_read_name_what_failed() {
    for (key, value, what) in [
        ("oiCaps", json!({}), "oi_caps"),
        ("oiCaps", json!([[0]]), "oi_cap_tuple"),
        ("oiCaps", json!([["BTC", "1"]]), "oi_cap_market"),
        ("oiCaps", json!([[0, "-1"]]), "oi_cap_amount"),
        ("oiCaps", json!([[0, "1"], [0, "2"]]), "oi_cap_duplicate"),
        ("oiCaps", json!([[null, "1"], [null, null]]), "oi_cap_duplicate"),
        ("recentNonces", json!([1]), "recent_nonces"),
        ("recentNonces", json!({"session": "0x1"}), "recent_nonce"),
    ] {
        let got = edited(|frame| frame[key] = value.clone());
        assert!(matches!(&got, Err(AccountError::Wire(bad)) if bad.what == what), "{key} {value}: {got:?}");
    }
}
