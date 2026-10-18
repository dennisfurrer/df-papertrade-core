//! What a session key may sign, and its signatures against viem's vectors.

use df_hyperliquid_core::utils::hex;

use super::*;
use crate::account::session_key::SESSION_TTL_MS;
use crate::test_support::{NOW_MS, SESSION_ADDRESS, SESSION_SECRET, owner, session_address, session_secret};
use crate::trade::intent::vectors::{cases, text, vectors};
use crate::trade::intent::{Claim, OpenPosition, RegisterSessionKey, Staking, WithdrawToCore};

#[test]
fn a_session_key_signs_only_its_wallets_opens_closes_and_cancels_in_time() {
    let held = NOW_MS + SESSION_TTL_MS;
    let deadline = NOW_MS / 1000 + 60;
    let open = Intent::OpenPosition(OpenPosition::new(owner(), 0, true, 10, 1000, false, 1, deadline).unwrap());
    let staking = Staking::new(owner(), 1, 1, deadline).unwrap();
    let register = RegisterSessionKey::new(owner(), session_address(), deadline + 1, 1, deadline).unwrap();
    let cases = [
        ("an open, now", &open, owner(), held, NOW_MS, Ok(())),
        ("a second before its deadline", &open, owner(), held, NOW_MS + 59_999, Ok(())),
        ("at its deadline", &open, owner(), held, NOW_MS + 60_000, Err(SessionRefusal::Expired)),
        ("with the key at its end", &open, owner(), NOW_MS, NOW_MS, Err(SessionRefusal::KeyExpired)),
        (
            "for another wallet",
            &open,
            session_address(),
            held,
            NOW_MS,
            Err(SessionRefusal::OtherWallet { intent: owner(), wallet: session_address() }),
        ),
        (
            "a claim",
            &Intent::Claim(Claim { user: owner(), nonce: 1, deadline }),
            owner(),
            held,
            NOW_MS,
            Err(SessionRefusal::NotSessionIntent("Claim")),
        ),
        (
            "a stake",
            &Intent::Stake(staking.clone()),
            owner(),
            held,
            NOW_MS,
            Err(SessionRefusal::NotSessionIntent("Stake")),
        ),
        (
            "an unstake",
            &Intent::Unstake(staking),
            owner(),
            held,
            NOW_MS,
            Err(SessionRefusal::NotSessionIntent("Unstake")),
        ),
        (
            "a withdrawal",
            &Intent::WithdrawToCore(WithdrawToCore::new(owner(), 10_000_000, 1, deadline).unwrap()),
            owner(),
            held,
            NOW_MS,
            Err(SessionRefusal::NotSessionIntent("WithdrawToCore")),
        ),
        (
            "its own registration",
            &Intent::RegisterSessionKey(register),
            owner(),
            held,
            NOW_MS,
            Err(SessionRefusal::NotSessionIntent("RegisterSessionKey")),
        ),
    ];
    let key = SessionKey::from_secret(&session_secret(), owner(), held).unwrap();
    for (name, intent, wallet, held, now, expected) in cases {
        assert_eq!(may_sign(intent, wallet, held, now), expected, "{name}");
        if wallet == owner() && held == key.held_until_ms() {
            assert_eq!(key.sign(intent.clone(), now).map(|_| ()), expected, "{name}: the key refuses it too");
        }
    }
}

/// RFC 6979 signatures are deterministic, so every session-signed case matches viem's exactly.
#[test]
fn the_session_key_signs_as_viem_does() {
    let vectors = vectors();
    let key = SessionKey::from_secret(&session_secret(), owner(), u64::MAX).unwrap();
    assert_eq!(key.address(), SESSION_ADDRESS.parse::<Address>().unwrap(), "the vectors' session key");
    let mut signed = 0;
    for (case, intent) in cases(&vectors).filter(|(case, _)| text(case, "signer") == "session") {
        let name = text(case, "name");
        let ours = key.sign(intent, NOW_MS).unwrap();
        assert_eq!(ours.signer(), key.address(), "{name}");
        assert_eq!(hex::encode_prefixed(&ours.intent_id()), text(case, "intentId"), "{name}");
        assert_eq!(
            (ours.signature().r_hex(), ours.signature().s_hex()),
            (text(case, "r").to_owned(), text(case, "s").to_owned())
        );
        signed += 1;
    }
    assert_eq!(signed, 7, "three opens, three closes and a cancel");
}

#[test]
fn a_session_key_prints_its_address_never_its_secret() {
    let key = SessionKey::from_secret(&session_secret(), owner(), 9).unwrap();
    let printed = format!("{key:?}");
    assert_eq!(
        printed,
        format!("SessionKey {{ address: {:?}, wallet: {:?}, held_until_ms: 9 }}", key.address(), owner())
    );
    assert!(!printed.contains(&SESSION_SECRET[2..10]));
}
