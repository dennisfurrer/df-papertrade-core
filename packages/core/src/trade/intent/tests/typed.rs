//! Every intent's struct, domain and digest checked against viem's vectors.

use df_hyperliquid_core::utils::hex;
use df_hyperliquid_core::utils::sign::digest_of_wallet_json;
use serde_json::Value;

use super::*;
use crate::test_support::{owner, session_address};
use crate::trade::intent::vectors::{address, cases, intent_of, text, vectors};
use crate::trade::intent::{deadline, intent_id};

#[test]
fn every_case_hashes_as_viem_does() {
    let vectors = vectors();
    let mut seen = 0;
    for (case, intent) in cases(&vectors) {
        let name = text(case, "name");
        let typed = intent.typed();
        assert_eq!(typed.type_signature(), text(case, "typeString"), "{name}");
        assert_eq!(hex::encode_prefixed(&typed.type_hash()), text(case, "typeHash"), "{name}");
        assert_eq!(hex::encode_prefixed(&typed.hash()), text(case, "structHash"), "{name}");
        let domain = &vectors["domains"][text(case, "domain")];
        assert_eq!(hex::encode_prefixed(&intent.domain().separator()), text(domain, "separator"), "{name}");
        assert_eq!(hex::encode_prefixed(&intent.digest()), text(case, "digest"), "{name}");
        seen += 1;
    }
    assert_eq!(seen, 12, "every case but Distribute");
}

/// viem writes `uint128` and `uint256` as decimal strings. Both JSONs must hash to the digest.
#[test]
fn both_wallet_jsons_read_back_to_the_digest() {
    let vectors = vectors();
    for (case, intent) in cases(&vectors) {
        let name = text(case, "name");
        let digest = hex::encode_prefixed(&intent.digest());
        assert_eq!(hex::encode_prefixed(&digest_of_wallet_json(&intent.wallet_json()).unwrap()), digest, "{name}");
        let viem = digest_of_wallet_json(text(case, "walletJson")).unwrap();
        assert_eq!(hex::encode_prefixed(&viem), digest, "{name}: viem's JSON");
    }
}

#[test]
fn a_big_number_reaches_the_wallet_as_a_string() {
    let open = OpenPosition::new(Address::ZERO, 0, true, u128::MAX, 1, false, 1, 2).unwrap();
    let json: Value = serde_json::from_str(&Intent::OpenPosition(open).wallet_json()).unwrap();
    let message = &json["message"];
    assert_eq!(message["size"], Value::String(u128::MAX.to_string()));
    assert_eq!(message["nonce"], Value::String("1".to_owned()));
    assert!(message["instrumentId"].is_number() && message["leverage"].is_number() && message["deadline"].is_number());
}

#[test]
fn each_intent_is_signed_by_its_key() {
    let vectors = vectors();
    for (case, intent) in cases(&vectors) {
        let expected = if text(case, "signer") == "session" { Key::Session } else { Key::Wallet };
        assert_eq!(intent.key(), expected, "{}", text(case, "name"));
    }
}

#[test]
fn the_domain_and_the_signer_change_the_answer() {
    let vectors = vectors();
    let open =
        vectors["cases"].as_array().unwrap().iter().find(|case| text(case, "primaryType") == "OpenPosition").unwrap();
    let intent = intent_of(open).unwrap();
    for rejected in vectors["rejected"].as_array().unwrap() {
        if rejected.get("domain").is_none() {
            let signer = address(rejected, "signer");
            assert_eq!(hex::encode_prefixed(&intent_id(&intent.digest(), signer)), text(rejected, "intentId"));
            assert_ne!(text(rejected, "intentId"), text(open, "intentId"));
            continue;
        }
        let d = &rejected["domain"];
        let name: &'static str = Box::leak(text(d, "name").to_owned().into_boxed_str());
        let version: &'static str = Box::leak(text(d, "version").to_owned().into_boxed_str());
        let domain = Domain {
            name,
            version,
            chain_id: d["chainId"].as_u64().unwrap(),
            verifying_contract: address(d, "verifyingContract"),
        };
        assert_eq!(hex::encode_prefixed(&domain.separator()), text(rejected, "separator"));
        let digest = intent.typed().digest(&domain);
        assert_eq!(hex::encode_prefixed(&digest), text(rejected, "digest"));
        assert_ne!(digest, intent.digest(), "{}", text(rejected, "why"));
    }
}

#[test]
fn every_intent_knows_its_wallet_deadline_nonce_action_key_and_domain() {
    let user = owner();
    let staking = Staking::new(user, 5, 4, 9).unwrap();
    let register = RegisterSessionKey::new(user, session_address(), 10, 4, 9).unwrap();
    let cases = [
        (Intent::RegisterSessionKey(register), Some("registerSessionKey"), Key::Wallet, EXCHANGE_DOMAIN),
        (
            Intent::OpenPosition(OpenPosition::new(user, 1, false, 10, 2, true, 4, 9).unwrap()),
            Some("openPosition"),
            Key::Session,
            EXCHANGE_DOMAIN,
        ),
        (Intent::Close(Close::new(user, &[1], 4, 9).unwrap()), Some("closePositions"), Key::Session, EXCHANGE_DOMAIN),
        (
            Intent::WithdrawToCore(WithdrawToCore::new(user, 10_000_000, 4, 9).unwrap()),
            Some("withdrawToCore"),
            Key::Wallet,
            EXCHANGE_DOMAIN,
        ),
        (Intent::Stake(staking.clone()), Some("paperStake"), Key::Wallet, STAKING_DOMAIN),
        (Intent::Unstake(staking), Some("paperUnstake"), Key::Wallet, STAKING_DOMAIN),
        (Intent::Claim(Claim { user, nonce: 4, deadline: 9 }), Some("paperClaim"), Key::Wallet, STAKING_DOMAIN),
    ];
    for (intent, action, key, domain) in cases {
        let name = intent.primary_type();
        assert_eq!((intent.user(), intent.deadline(), intent.nonce()), (user, 9, Some(4)), "{name}");
        assert_eq!((intent.action(), intent.key(), intent.domain()), (action, key, domain), "{name}");
        assert!(intent.typed().type_signature().starts_with(&format!("{name}(")), "{name}");
    }
    let cancel = Intent::CancelIntent(CancelIntent { user, intent_id: [1; 32], deadline: 9 });
    assert_eq!((cancel.user(), cancel.deadline(), cancel.nonce()), (user, 9, None), "a cancel spends no nonce");
    assert_eq!((cancel.action(), cancel.key(), cancel.domain()), (None, Key::Session, CANCEL_DOMAIN));
}

#[test]
fn an_intent_made_now_lives_an_hour() {
    for (now_ms, expected) in [(0, 3_600), (999, 3_600), (1_791_860_400_123, 1_791_864_000)] {
        assert_eq!(deadline(now_ms), expected, "{now_ms}");
    }
}
