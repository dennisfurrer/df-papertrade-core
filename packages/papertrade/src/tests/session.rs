use hl_core::{Address, hex};

use super::*;
use crate::intent::{Claim, Close, OpenPosition};

const WALLET: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
const SESSION_SECRET: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const SESSION: &str = "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23";
const NOW_MS: u64 = 1_791_860_400_000;

fn wallet() -> Address {
    WALLET.parse().unwrap()
}

fn key() -> Address {
    SESSION.parse().unwrap()
}

#[test]
fn a_key_lasts_thirty_days() {
    assert_eq!(expiry(NOW_MS), (NOW_MS + 30 * 24 * 3_600_000) / 1000);
}

#[test]
fn the_status_reads_the_stream_and_the_clock() {
    let held = NOW_MS + SESSION_TTL_MS;
    let registered = [(key(), held / 1000)];
    assert_eq!(status(key(), held, &[], 0, NOW_MS), Status::Unregistered);
    assert_eq!(status(key(), held, &registered, 0, NOW_MS), Status::Ready { expires_at: held / 1000 });
    let late = held - RENEW_BEFORE_MS;
    assert_eq!(status(key(), held, &registered, 0, late), Status::Expiring { expires_at: held / 1000 });
    assert_eq!(
        status(key(), held, &registered, late / 1000, NOW_MS),
        Status::Expiring { expires_at: held / 1000 },
        "the indexer's time counts when it is ahead of the clock"
    );
    let shorter = [(key(), NOW_MS / 1000 + 7_200)];
    assert_eq!(status(key(), held, &shorter, 0, NOW_MS), Status::Ready { expires_at: NOW_MS / 1000 + 7_200 });
}

#[test]
fn a_registration_runs_until_the_key_does() {
    let held = NOW_MS + SESSION_TTL_MS;
    let registration = registration(wallet(), key(), held, 3_170_816, NOW_MS).unwrap();
    assert_eq!(registration.expiry, held / 1000);
    assert_eq!(registration.deadline, NOW_MS / 1000 + 3_600);
    assert_eq!(registration.fee_address, Address::ZERO);
}

#[test]
fn a_session_key_signs_only_what_it_may() {
    let held = NOW_MS + SESSION_TTL_MS;
    let open =
        Intent::OpenPosition(OpenPosition::new(wallet(), 0, true, 10, 1000, false, 1, NOW_MS / 1000 + 60).unwrap());
    assert_eq!(may_sign(&open, wallet(), held, NOW_MS), Ok(()));
    let claim = Intent::Claim(Claim { user: wallet(), nonce: 1, deadline: NOW_MS / 1000 + 60 });
    assert_eq!(may_sign(&claim, wallet(), held, NOW_MS), Err(SessionRefusal::NotSessionIntent("Claim")));
    assert!(matches!(may_sign(&open, key(), held, NOW_MS), Err(SessionRefusal::OtherWallet { .. })));
    assert_eq!(may_sign(&open, wallet(), held, NOW_MS + 61_000), Err(SessionRefusal::Expired));
    assert_eq!(may_sign(&open, wallet(), NOW_MS, NOW_MS), Err(SessionRefusal::KeyExpired));
}

#[test]
fn the_session_key_signs_as_viem_does() {
    let secret: [u8; 32] = hex::decode_array(SESSION_SECRET).unwrap();
    let session = SessionKey::from_secret(&secret, wallet(), u64::MAX).unwrap();
    assert_eq!(session.address(), key());
    assert!(!format!("{session:?}").contains(&SESSION_SECRET[2..]), "the secret never prints");
    let close = Intent::Close(Close::new(wallet(), &[408_665], 3_170_820, 1_791_864_000).unwrap());
    let signed = session.sign(close, NOW_MS).unwrap();
    assert_eq!(signed.signer(), key());
    // `close one position` in vectors/papertrade_viem.json.
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../../vectors/papertrade_viem.json")).unwrap();
    let case = vectors["cases"].as_array().unwrap().iter().find(|case| case["name"] == "close one position").unwrap();
    assert_eq!(hex::encode_prefixed(&signed.intent_id()), case["intentId"].as_str().unwrap());
    assert_eq!(signed.signature().r_hex(), case["r"].as_str().unwrap());
}
