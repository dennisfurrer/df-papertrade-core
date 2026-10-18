//! What a session key may sign, and its signatures against viem's vectors.

use df_hyperliquid_core::utils::hex;

use super::*;
use crate::account::session_key::SESSION_TTL_MS;
use crate::test_support::{NOW_MS, SESSION_ADDRESS, SESSION_SECRET, owner, session_address, session_secret};
use crate::trade::intent::vectors::vectors;
use crate::trade::intent::{Claim, Close, OpenPosition};

#[test]
fn a_session_key_signs_only_what_it_may() {
    let held = NOW_MS + SESSION_TTL_MS;
    let open =
        Intent::OpenPosition(OpenPosition::new(owner(), 0, true, 10, 1000, false, 1, NOW_MS / 1000 + 60).unwrap());
    assert_eq!(may_sign(&open, owner(), held, NOW_MS), Ok(()));
    let claim = Intent::Claim(Claim { user: owner(), nonce: 1, deadline: NOW_MS / 1000 + 60 });
    assert_eq!(may_sign(&claim, owner(), held, NOW_MS), Err(SessionRefusal::NotSessionIntent("Claim")));
    assert!(matches!(may_sign(&open, session_address(), held, NOW_MS), Err(SessionRefusal::OtherWallet { .. })));
    assert_eq!(may_sign(&open, owner(), held, NOW_MS + 61_000), Err(SessionRefusal::Expired));
    assert_eq!(may_sign(&open, owner(), NOW_MS, NOW_MS), Err(SessionRefusal::KeyExpired));
}

#[test]
fn the_session_key_signs_as_viem_does() {
    let key: Address = SESSION_ADDRESS.parse().unwrap();
    let session = SessionKey::from_secret(&session_secret(), owner(), u64::MAX).unwrap();
    assert_eq!(session.address(), key);
    assert!(!format!("{session:?}").contains(&SESSION_SECRET[2..]), "the secret never prints");
    let close = Intent::Close(Close::new(owner(), &[408_665], 3_170_820, 1_791_864_000).unwrap());
    let signed = session.sign(close, NOW_MS).unwrap();
    assert_eq!(signed.signer(), key);
    let vectors = vectors();
    let case = vectors["cases"].as_array().unwrap().iter().find(|case| case["name"] == "close one position").unwrap();
    assert_eq!(hex::encode_prefixed(&signed.intent_id()), case["intentId"].as_str().unwrap());
    assert_eq!(signed.signature().r_hex(), case["r"].as_str().unwrap());
}
