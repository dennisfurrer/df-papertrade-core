//! viem's vectors (`vectors/papertrade_viem.json`) and the readers that turn each case into its
//! intent, signature and signer.

use df_hyperliquid_core::account::{LocalKey, Signature};
use df_hyperliquid_core::utils::sign::eip712::decimal_word;
use df_hyperliquid_core::utils::{Address, hex};
use serde_json::Value;

use super::{CancelIntent, Claim, Close, Intent, OpenPosition, RegisterSessionKey, Staking, WithdrawToCore};

const VECTORS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/papertrade_viem.json"));

pub(crate) fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_else(|| panic!("`{key}` is not a string in {value}"))
}

pub(crate) fn address(value: &Value, key: &str) -> Address {
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

/// `None` for Distribute, which the keeper sends rather than a wallet.
pub(crate) fn intent_of(case: &Value) -> Option<Intent> {
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

pub(crate) fn signature_of(case: &Value) -> Signature {
    let v = u8::try_from(case["v"].as_u64().unwrap()).unwrap();
    Signature::from_parts(word(text(case, "r")), word(text(case, "s")), v)
}

pub(crate) fn signer_of(vectors: &Value, case: &Value) -> (LocalKey, Address) {
    let who = &vectors[text(case, "signer")];
    (LocalKey::from_hex(text(who, "key")).unwrap(), address(who, "address"))
}

pub(crate) fn cases(vectors: &Value) -> impl Iterator<Item = (&Value, Intent)> {
    vectors["cases"].as_array().unwrap().iter().filter_map(|case| intent_of(case).map(|intent| (case, intent)))
}
