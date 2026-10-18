//! Every intent's struct, domain and digest checked against viem's vectors.

use df_hyperliquid_core::utils::hex;
use df_hyperliquid_core::utils::sign::digest_of_wallet_json;
use serde_json::Value;

use super::*;
use crate::test_support::owner;
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
fn an_intent_knows_its_place() {
    let user = owner();
    let open = Intent::OpenPosition(OpenPosition::new(user, 1, false, 10, 2, true, 5, 9).unwrap());
    assert_eq!((open.user(), open.deadline(), open.nonce()), (user, 9, Some(5)));
    assert_eq!(open.domain(), EXCHANGE_DOMAIN);
    let cancel = Intent::CancelIntent(CancelIntent { user, intent_id: [1; 32], deadline: 9 });
    assert_eq!((cancel.nonce(), cancel.action()), (None, None));
    assert_eq!(deadline(1_791_860_400_123), 1_791_864_000);
}
