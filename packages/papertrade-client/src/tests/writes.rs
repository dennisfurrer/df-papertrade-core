use hl_core::Method;
use hl_papertrade::intent::{Close, Intent, OpenPosition, Staking, WithdrawToCore, deadline, intent_id};
use hl_papertrade::math::OpenRefusal;

use super::*;
use crate::{Error, KeyState, Open};

fn ticket() -> Open {
    Open { instrument_id: 0, is_long: true, margin: 10 * WAD, leverage: 1000, use_debt: false }
}

#[test]
fn an_open_is_checked_then_signed_by_the_session_key() {
    let store = store(Some(&record(true)), 77, 1);
    let open = OpenPosition::new(owner(), 0, true, 10 * WAD, 1000, false, 77, deadline(NOW_MS)).unwrap();
    let id = intent_id(&Intent::OpenPosition(open).digest(), session_address());
    let wire = Wire::default().on(Method::Get, "/api/papertrade/state/trading", "", 200, TRADING).on(
        Method::Post,
        "/api/papertrade/intents",
        "",
        200,
        &accepted(id, "openPosition"),
    );
    let client = client(&wire);
    let account = account("");

    let answer = block_on(client.open(&store, owner(), &account, ticket(), Some(829_105))).unwrap();
    assert_eq!(answer.intent_id, id);
    let posted = wire.posts_to("/api/papertrade/intents");
    let body = body(&posted[0]);
    assert_eq!(body["signed"]["size"], (10 * WAD).to_string());
    assert_eq!(body["signed"]["leverage"], "1000");

    let too_much = Open { margin: 200 * WAD, leverage: 10, ..ticket() };
    assert_eq!(
        block_on(client.open(&store, owner(), &account, too_much, Some(829_105))).unwrap_err(),
        Error::Open(OpenRefusal::InsufficientFunds)
    );
    let tiny = Open { margin: WAD, ..ticket() };
    assert!(matches!(block_on(client.open(&store, owner(), &account, tiny, Some(829_105))), Err(Error::Open(_))));
    assert_eq!(wire.posts_to("/api/papertrade/intents").len(), 1, "refusals send nothing");
}

#[test]
fn an_open_without_a_session_key_names_the_step() {
    let store = store(None, 1, 1);
    let wire = Wire::default().on(Method::Get, "/api/papertrade/state/trading", "", 200, TRADING);
    let e = block_on(client(&wire).open(&store, owner(), &account(""), ticket(), Some(829_105))).unwrap_err();
    assert_eq!(e, Error::NeedsKey(KeyState::Missing));
    let unregistered = super::store(Some(&record(false)), 1, 1);
    let e = block_on(client(&wire).open(&unregistered, owner(), &account(""), ticket(), Some(829_105))).unwrap_err();
    assert_eq!(e, Error::NeedsKey(KeyState::Unregistered));
}

#[test]
fn closes_go_twelve_to_an_intent() {
    let store = store(Some(&record(true)), 500, 1);
    let ids: Vec<u128> = (1..=25).collect();
    let mut wire = Wire::default();
    for (n, chunk) in ids.chunks(12).enumerate() {
        let close = Close::new(owner(), chunk, 500 + n as u128, deadline(NOW_MS)).unwrap();
        let id = intent_id(&Intent::Close(close).digest(), session_address());
        let piece: &'static str = Box::leak(format!("\"nonce\":\"{}\"", 500 + n).into_boxed_str());
        wire = wire.on(Method::Post, "/api/papertrade/intents", piece, 200, &accepted(id, "closePositions"));
    }
    let answers = block_on(client(&wire).close(&store, owner(), None, &ids)).unwrap();
    assert_eq!(answers.len(), 3);
    assert_eq!(wire.posts_to("/api/papertrade/intents").len(), 3);
    assert_eq!(
        block_on(client(&wire).close(&store, owner(), None, &[])).unwrap_err(),
        Error::Invalid("no positions to close".into())
    );
}

#[test]
fn a_withdrawal_over_the_balance_sends_nothing_and_one_within_it_is_the_wallets() {
    let store = store(None, 40, 1);
    let account = account("");
    let wire = Wire::default();
    let over = block_on(client(&wire).withdraw(&store, &wallet(), Some(&account), 101_000_000)).unwrap_err();
    assert!(matches!(over, Error::InsufficientFunds(_)));
    assert!(wire.sent().is_empty());

    let intent = WithdrawToCore::new(owner(), 50_000_000, 40, deadline(NOW_MS)).unwrap();
    let id = intent_id(&Intent::WithdrawToCore(intent).digest(), owner());
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &accepted(id, "withdrawToCore"));
    let answer = block_on(client(&wire).withdraw(&store, &wallet(), Some(&account), 50_000_000)).unwrap();
    assert_eq!(answer.intent_id, id, "signed by the wallet itself, not a session key");
}

#[test]
fn staking_spends_its_own_lane() {
    let store = store(None, 40, 900);
    let stake = Staking::new(owner(), 5 * WAD, 900, deadline(NOW_MS)).unwrap();
    let id = intent_id(&Intent::Stake(stake).digest(), owner());
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &accepted(id, "stake"));
    let answer = block_on(client(&wire).stake(&store, &wallet(), 5 * WAD)).unwrap();
    assert_eq!(answer.intent_id, id);
    assert_eq!(block_on(client(&wire).take_nonce(&store, owner(), Lane::Exchange)).unwrap(), 40, "untouched");
}
