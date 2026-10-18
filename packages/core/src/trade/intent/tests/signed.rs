//! Signed intents checked against viem's vectors: payloads, intent ids, signatures and bodies.

use serde_json::Value;

use super::*;
use crate::trade::intent::vectors::{cases, signature_of, signer_of, text, vectors};

#[test]
fn the_payload_and_intent_id_are_the_apps() {
    let vectors = vectors();
    for (case, intent) in cases(&vectors) {
        let name = text(case, "name");
        let (_, signer) = signer_of(&vectors, case);
        let signed = intent.signed(signature_of(case), signer);
        let payload: Value = serde_json::from_str(&signed.payload().compact()).unwrap();
        assert_eq!(payload, case["signed"], "{name}");
        let ordered = signed.payload();
        let keys: Vec<&str> = ordered.as_object().unwrap().iter().map(|(key, _)| key.as_str()).collect();
        let mut expected: Vec<&str> =
            case["types"].as_array().unwrap().iter().map(|field| text(field, "name")).collect();
        expected.extend(["v", "r", "s"]);
        assert_eq!(keys, expected, "{name}: fields in the type's order");
        assert_eq!(hex::encode_prefixed(&signed.intent_id()), text(case, "intentId"), "{name}");
    }
}

/// RFC 6979 signatures are deterministic, so they match viem's exactly.
#[test]
fn the_same_key_signs_the_same_signature() {
    let vectors = vectors();
    for (case, intent) in cases(&vectors) {
        let name = text(case, "name");
        let (key, signer) = signer_of(&vectors, case);
        assert_eq!(key.address(), signer);
        let signature = key.sign_digest(&intent.digest()).unwrap();
        assert_eq!(signature, signature_of(case), "{name}");
        let signed = Signed::recover(intent.clone(), signature, signer).unwrap();
        assert_eq!(signed.signer(), signer);
        let stranger = "0x0000000000000000000000000000000000000001".parse().unwrap();
        assert!(matches!(Signed::recover(intent, signature, stranger), Err(BadSignature::WrongSigner { .. })));
    }
}

#[test]
fn the_tag_is_the_apps() {
    assert_eq!(hex::encode_prefixed(&intent_id_tag()), text(&vectors(), "intentIdTag"));
}

#[test]
fn each_body_is_what_the_relayer_takes_at_its_path() {
    let vectors = vectors();
    let actions = [
        ("RegisterSessionKey", "registerSessionKey"),
        ("OpenPosition", "openPosition"),
        ("Close", "closePositions"),
        ("WithdrawToCore", "withdrawToCore"),
        ("Stake", "paperStake"),
        ("Unstake", "paperUnstake"),
        ("Claim", "paperClaim"),
    ];
    for (case, intent) in cases(&vectors) {
        let name = text(case, "name");
        let (_, signer) = signer_of(&vectors, case);
        let signed = intent.signed(signature_of(case), signer);
        let body: Value = serde_json::from_str(&signed.body()).unwrap();
        if text(case, "primaryType") == "CancelIntent" {
            let target = text(&case["message"], "intentId");
            assert_eq!(signed.path(), format!("/intents/{target}/cancel"), "{name}");
            let mut expected = case["signed"].clone();
            expected.as_object_mut().unwrap().remove("intentId");
            assert_eq!(body, expected, "{name}: the payload without the id the path names");
            continue;
        }
        let (_, action) = actions.iter().find(|(kind, _)| *kind == text(case, "primaryType")).unwrap();
        let mut expected = serde_json::json!({"action": action, "signed": case["signed"]});
        if *action == "withdrawToCore" {
            expected["withdrawal"] = serde_json::json!({"route": "hypercore"});
        }
        assert_eq!((signed.path(), body), ("/intents".to_owned(), expected), "{name}");
    }
}
