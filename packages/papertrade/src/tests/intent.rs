//! Every intent against viem (`vectors/papertrade_viem.json`): the type
//! string and hash, the struct hash, the digest under its domain, the wallet
//! JSON (both ours and viem's read back to the same digest), the relayer's
//! payload field for field, the intent id, and the signature itself from the
//! same key.

use hl_core::{
    Address, Domain, LocalKey, Signature, digest_of_wallet_json,
    eip712::{decimal_word, uint256},
    hex,
};
use serde_json::Value;

use super::*;
use crate::deployment::{EXCHANGE, EXCHANGE_DOMAIN};

const VECTORS: &str = include_str!("../../../../vectors/papertrade_viem.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_else(|| panic!("`{key}` is not a string in {value}"))
}

fn address(value: &Value, key: &str) -> Address {
    text(value, key).parse().unwrap()
}

fn number<T: std::str::FromStr>(value: &Value, key: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    text(value, key).parse().unwrap()
}

fn word(text: &str) -> [u8; 32] {
    hex::decode_array(text).unwrap()
}

/// The intent a case's message describes, or `None` for one the SDK does not
/// build (Distribute: PaperStaking takes it from the keeper, not a wallet).
fn intent_of(case: &Value) -> Option<Intent> {
    let m = &case["message"];
    let user = address(m, "user");
    Some(match text(case, "primaryType") {
        "RegisterSessionKey" => {
            assert_eq!(address(m, "feeAddress"), Address::ZERO);
            Intent::RegisterSessionKey(
                RegisterSessionKey::new(
                    user,
                    address(m, "key"),
                    number(m, "expiry"),
                    number(m, "nonce"),
                    number(m, "deadline"),
                )
                .unwrap(),
            )
        }
        "OpenPosition" => Intent::OpenPosition(
            OpenPosition::new(
                user,
                number(m, "instrumentId"),
                m["isLong"].as_bool().unwrap(),
                number(m, "size"),
                number(m, "leverage"),
                m["useDebt"].as_bool().unwrap(),
                number(m, "nonce"),
                number(m, "deadline"),
            )
            .unwrap(),
        ),
        "Close" => {
            let ids = m["positionIds"].as_array().unwrap();
            let words = ids.iter().map(|id| decimal_word(id.as_str().unwrap()).unwrap()).collect();
            Intent::Close(Close::of_words(user, words, number(m, "nonce"), number(m, "deadline")).unwrap())
        }
        "WithdrawToCore" => Intent::WithdrawToCore(
            WithdrawToCore::new(user, number(m, "amount"), number(m, "nonce"), number(m, "deadline")).unwrap(),
        ),
        "Stake" => {
            Intent::Stake(Staking::new(user, number(m, "amount"), number(m, "nonce"), number(m, "deadline")).unwrap())
        }
        "Unstake" => {
            Intent::Unstake(Staking::new(user, number(m, "amount"), number(m, "nonce"), number(m, "deadline")).unwrap())
        }
        "Claim" => Intent::Claim(Claim { user, nonce: number(m, "nonce"), deadline: number(m, "deadline") }),
        "CancelIntent" => Intent::CancelIntent(CancelIntent {
            user,
            intent_id: word(text(m, "intentId")),
            deadline: number(m, "deadline"),
        }),
        "Distribute" => return None,
        other => panic!("no intent for {other}"),
    })
}

fn signature_of(case: &Value) -> Signature {
    let v = u8::try_from(case["v"].as_u64().unwrap()).unwrap();
    Signature::from_parts(word(text(case, "r")), word(text(case, "s")), v)
}

fn signer_of(vectors: &Value, case: &Value) -> (LocalKey, Address) {
    let who = &vectors[text(case, "signer")];
    (LocalKey::from_hex(text(who, "key")).unwrap(), address(who, "address"))
}

fn cases(vectors: &Value) -> impl Iterator<Item = (&Value, Intent)> {
    vectors["cases"].as_array().unwrap().iter().filter_map(|case| intent_of(case).map(|intent| (case, intent)))
}

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

/// What a wallet hashes from our typed data, and from viem's (which writes a
/// `uint128` or `uint256` as a decimal string), is the digest.
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

/// RFC 6979 on both sides: the same key signs the same bytes.
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
fn the_tag_is_the_apps() {
    assert_eq!(hex::encode_prefixed(&intent_id_tag()), text(&vectors(), "intentIdTag"));
}

#[test]
fn a_body_is_what_the_relayer_takes() {
    let vectors = vectors();
    let find = |kind: &str| {
        let case = vectors["cases"].as_array().unwrap().iter().find(|case| text(case, "primaryType") == kind).unwrap();
        let (_, signer) = signer_of(&vectors, case);
        (case, intent_of(case).unwrap().signed(signature_of(case), signer))
    };

    let (case, open) = find("OpenPosition");
    assert_eq!(open.path(), "/intents");
    let body: Value = serde_json::from_str(&open.body()).unwrap();
    assert_eq!(body["action"], "openPosition");
    assert_eq!(body["signed"], case["signed"]);
    assert_eq!(body.as_object().unwrap().len(), 2);

    let (_, withdraw) = find("WithdrawToCore");
    let body: Value = serde_json::from_str(&withdraw.body()).unwrap();
    assert_eq!(body["action"], "withdrawToCore");
    assert_eq!(body["withdrawal"], serde_json::json!({"route": "hypercore"}));

    let (case, cancel) = find("CancelIntent");
    let target = text(&case["message"], "intentId");
    assert_eq!(cancel.path(), format!("/intents/{target}/cancel"));
    let body: Value = serde_json::from_str(&cancel.body()).unwrap();
    let mut expected = case["signed"].clone();
    expected.as_object_mut().unwrap().remove("intentId");
    assert_eq!(body, expected, "the payload without the id the path names");

    for (kind, action) in [
        ("RegisterSessionKey", "registerSessionKey"),
        ("Close", "closePositions"),
        ("Stake", "paperStake"),
        ("Unstake", "paperUnstake"),
        ("Claim", "paperClaim"),
    ] {
        let (_, signed) = find(kind);
        let body: Value = serde_json::from_str(&signed.body()).unwrap();
        assert_eq!(body["action"], action);
    }
}

#[test]
fn the_builders_refuse_what_the_venue_refuses() {
    let user: Address = "0x14791697260e4c9a71f18484c9f997b308e59325".parse().unwrap();
    assert_eq!(OpenPosition::new(user, 0, true, 1, 0, false, 1, 2), Err(Refusal::Leverage(0)));
    assert_eq!(OpenPosition::new(user, 0, true, 1, 1001, false, 1, 2), Err(Refusal::Leverage(1001)));
    assert_eq!(OpenPosition::new(user, 0, true, 0, 10, false, 1, 2), Err(Refusal::NoMargin));
    assert!(OpenPosition::new(user, 0, true, 1, MAX_LEVERAGE, false, 1, 2).is_ok());

    assert_eq!(Close::new(user, &[], 1, 2), Err(Refusal::CloseCount(0)));
    let thirteen: Vec<u128> = (0..13).collect();
    assert_eq!(Close::new(user, &thirteen, 1, 2), Err(Refusal::CloseCount(13)));
    assert_eq!(Close::new(user, &[7, 8, 7], 1, 2), Err(Refusal::DuplicatePosition));
    let close = Close::new(user, &[408_665], 1, 2).unwrap();
    assert_eq!(close.position_ids(), &[uint256(408_665)]);

    assert_eq!(WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC - 1, 1, 2), Err(Refusal::WithdrawalTooSmall));
    assert!(WithdrawToCore::new(user, MIN_WITHDRAWAL_USDC, 1, 2).is_ok());
    assert_eq!(Staking::new(user, 0, 1, 2), Err(Refusal::NoAmount));

    assert_eq!(RegisterSessionKey::new(user, user, 10, 1, 2), Err(Refusal::BadSessionKey));
    assert_eq!(RegisterSessionKey::new(user, Address::ZERO, 10, 1, 2), Err(Refusal::BadSessionKey));
    assert_eq!(RegisterSessionKey::new(user, EXCHANGE, 2, 1, 2), Err(Refusal::ExpiresFirst));
}

#[test]
fn an_intent_knows_its_place() {
    let user: Address = "0x14791697260e4c9a71f18484c9f997b308e59325".parse().unwrap();
    let open = Intent::OpenPosition(OpenPosition::new(user, 1, false, 10, 2, true, 5, 9).unwrap());
    assert_eq!((open.user(), open.deadline(), open.nonce()), (user, 9, Some(5)));
    assert_eq!(open.domain(), EXCHANGE_DOMAIN);
    let cancel = Intent::CancelIntent(CancelIntent { user, intent_id: [1; 32], deadline: 9 });
    assert_eq!((cancel.nonce(), cancel.action()), (None, None));
    assert_eq!(deadline(1_791_860_400_123), 1_791_864_000);
}
