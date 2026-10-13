use hl_core::Method;
use hl_papertrade::intent::{Intent, RegisterSessionKey, deadline, intent_id};

use super::*;
use crate::{Error, KeyState, key_state};

#[test]
fn a_record_keeps_its_secret_and_times_in_48_bytes() {
    let kept = record(true);
    let bytes = kept.to_bytes();
    assert_eq!(bytes.len(), 48);
    assert_eq!(SessionRecord::from_bytes(&bytes), Some(kept.clone()));
    assert_eq!(SessionRecord::from_bytes(&record(false).to_bytes()).unwrap().registered_at_ms, None);
    assert_eq!(SessionRecord::from_bytes(&bytes[..47]), None);
    assert!(!format!("{kept:?}").contains("4c0883"), "the secret is never printed");
}

#[test]
fn the_key_state_reads_the_record_and_the_stream() {
    assert_eq!(key_state(None, None, NOW_MS), KeyState::Missing);
    assert_eq!(key_state(Some(&record(false)), None, NOW_MS), KeyState::Unregistered);
    assert_eq!(key_state(Some(&record(true)), None, NOW_MS), KeyState::Ready);
    assert_eq!(key_state(Some(&record(true)), None, NOW_MS + TTL - 60_000), KeyState::Missing, "too near its end");

    let listed = format!(r#"["{}","{}"]"#, session_address().to_hex(), (NOW_MS + TTL) / 1000);
    assert_eq!(
        key_state(Some(&record(false)), Some(&account(&listed)), NOW_MS),
        KeyState::Ready,
        "the stream lists it"
    );
    let unlisted = account("");
    assert_eq!(key_state(Some(&record(false)), Some(&unlisted), NOW_MS), KeyState::Unregistered);
    assert_eq!(key_state(Some(&record(true)), Some(&unlisted), NOW_MS), KeyState::Ready, "the indexer may lag");
    let long_ago = SessionRecord { registered_at_ms: Some(NOW_MS - 10 * 60_000), ..record(true) };
    assert_eq!(
        key_state(Some(&long_ago), Some(&unlisted), NOW_MS),
        KeyState::Unregistered,
        "taken ten minutes before the indexer's time and still not listed: it did not land"
    );
}

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

#[test]
fn another_wallet_cannot_sign_for_the_account() {
    let stranger = LocalWallet(LocalKey::from_bytes(&[9u8; 32]).unwrap());
    let wire = Wire::default();
    let intent = Intent::Claim(hl_papertrade::Claim { user: owner(), nonce: 1, deadline: deadline(NOW_MS) });
    let e = block_on(client(&wire).wallet_sign(&stranger, owner(), intent, crate::Purpose::Claim)).unwrap_err();
    assert!(matches!(e, Error::WrongWallet { .. }));
    assert!(wire.sent().is_empty());
}

#[test]
fn nonces_count_up_from_a_random_start() {
    let wire = Wire::default();
    let client = client(&wire);
    let store = MemoryStore::default();
    let first = block_on(client.take_nonce(&store, owner(), Lane::Exchange)).unwrap();
    assert_eq!(first, hl_papertrade::nonce::first([7, 7, 7]));
    assert_eq!(block_on(client.take_nonce(&store, owner(), Lane::Exchange)).unwrap(), first + 1);
    assert_eq!(block_on(client.take_nonce(&store, owner(), Lane::Staking)).unwrap(), first, "its own lane");
}
