//! Making, keeping and registering the session key over a scripted wire and an in-memory store.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::account::store::MemoryStore;
use crate::test_support::{
    NOW_MS, TTL, Wire, accepted, block_on, body, client, owner, record, session_address, store, wallet,
};
use crate::trade::intent::{RegisterSessionKey, deadline, intent_id};

#[test]
fn a_new_session_key_is_kept_for_thirty_days_and_reused() {
    let wire = Wire::default();
    let client = client(&wire);
    let store = MemoryStore::default();
    let made = block_on(client.new_session(&store, owner())).unwrap();
    assert_eq!(made.held_until_ms, NOW_MS + TTL);
    assert_eq!(made.registered_at_ms, None);
    assert_eq!(*made.secret, [7u8; 32], "from the caller's randomness");
    assert_eq!(block_on(client.session(&store, owner())).unwrap(), Some(made.clone()));
    assert_eq!(block_on(client.new_session(&store, owner())).unwrap(), made, "the same key, kept");
    assert_eq!(block_on(client.key_state(&store, owner(), None)).unwrap(), KeyState::Unregistered);
    assert!(wire.sent().is_empty(), "nothing sent");
}

#[test]
fn the_wallet_registers_the_key_and_the_relayer_is_asked_once() {
    let store = store(Some(&record(false)), 3_170_816, 1);
    let registration =
        RegisterSessionKey::new(owner(), session_address(), (NOW_MS + TTL) / 1000, 3_170_816, deadline(NOW_MS))
            .unwrap();
    let id = intent_id(&Intent::RegisterSessionKey(registration).digest(), owner());
    let wire =
        Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &accepted(id, "registerSessionKey"));
    let client = client(&wire);
    assert_eq!(block_on(client.enable(&store, &wallet(), None)).unwrap(), KeyState::Ready);
    let kept = block_on(client.session(&store, owner())).unwrap().unwrap();
    assert_eq!(kept.registered_at_ms, Some(NOW_MS));
    let sent = wire.sent();
    assert_eq!(sent.len(), 1, "posted once");
    let body = body(&sent[0]);
    assert_eq!(body["action"], "registerSessionKey");
    assert_eq!(body["signed"]["key"], session_address().to_hex());
    assert_eq!(body["signed"]["feeAddress"], "0x0000000000000000000000000000000000000000");
    assert_eq!(body["signed"]["nonce"], "3170816");

    assert_eq!(block_on(client.enable(&store, &wallet(), None)).unwrap(), KeyState::Ready);
    assert_eq!(wire.sent().len(), 1, "a ready key asks nothing again");
}

#[test]
fn a_registration_the_relayer_does_not_keep_is_not_recorded() {
    let store = store(Some(&record(false)), 9, 1);
    let refused = r#"{"intentId":"0x00","action":"registerSessionKey","state":"rejected"}"#;
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, refused);
    let client = client(&wire);
    assert!(block_on(client.register(&store, &wallet())).is_err());
    assert_eq!(block_on(client.session(&store, owner())).unwrap().unwrap().registered_at_ms, None);
}
