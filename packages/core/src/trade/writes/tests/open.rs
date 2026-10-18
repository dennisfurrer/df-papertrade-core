//! Opens over a scripted wire: checked first, signed by the session key, refused before sending.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::account::KeyState;
use crate::test_support::{
    NOW_MS, TRADING, WAD, Wire, accepted, account, block_on, body, client, owner, record, session_address, store,
};
use crate::trade::intent::intent_id;
use crate::trade::math::OpenRefusal;

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
    let unregistered = crate::test_support::store(Some(&record(false)), 1, 1);
    let e = block_on(client(&wire).open(&unregistered, owner(), &account(""), ticket(), Some(829_105))).unwrap_err();
    assert_eq!(e, Error::NeedsKey(KeyState::Unregistered));
}
