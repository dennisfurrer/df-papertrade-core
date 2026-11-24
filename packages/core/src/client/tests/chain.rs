//! Each read's contract, calldata and decoding: calldata from `cast calldata`, answers from the
//! mainnet RPC (2026-10-13 and 2026-10-18).

use df_hyperliquid_core::utils::{Address, hex};

use super::*;

const WALLET: &str = "0x9587c978341b4de61fa14e4705f65fbaaec72741";

fn word(text: &str) -> Vec<u8> {
    hex::decode(text).unwrap()
}

/// 2^128, one past what a `u128` holds.
const PAST_U128: &str = "0000000000000000000000000000000100000000000000000000000000000000";

#[test]
fn each_read_goes_to_its_contract_with_its_calldata() {
    let wallet: Address = WALLET.parse().unwrap();
    let padded = format!("000000000000000000000000{}", &WALLET[2..]);
    for ((to, data), contract, calldata, function) in [
        (
            parts(&deposit_address(wallet).unwrap()),
            DEPOSIT_PROXY_FACTORY,
            format!("0xd600539a{padded}"),
            "computeProxyAddress",
        ),
        (parts(&win_fee_rate().unwrap()), EXCHANGE, "0x48ee5e08".to_owned(), "winFeeRate"),
        (parts(&pending_reward(wallet).unwrap()), PAPER_STAKING, format!("0xf40f0f52{padded}"), "pendingReward"),
    ] {
        assert_eq!((to, hex::encode_prefixed(&data)), (contract, calldata), "{function}");
    }
    let read = format!("{:?}", win_fee_rate().unwrap());
    assert!(read.starts_with("Read { to: ") && read.ends_with(r#"function: "winFeeRate()" }"#), "{read}");
    let request = win_fee_rate().unwrap().request();
    assert_eq!((request.to, hex::encode_prefixed(&request.data)), (Some(EXCHANGE), "0x48ee5e08".to_owned()));
}

fn parts<T>(read: &Read<T>) -> (Address, Vec<u8>) {
    (read.to, read.data.clone())
}

#[test]
fn each_answer_decodes_to_its_value_or_says_why_not() {
    let wallet: Address = WALLET.parse().unwrap();
    let address = deposit_address(wallet).unwrap();
    let proxy = address.decode(&word("000000000000000000000000c125c9a929bd75555a027dd563c89a116bd41c50"));
    assert_eq!(proxy.map(|proxy| proxy.to_hex()), Ok("0xc125c9a929bd75555a027dd563c89a116bd41c50".to_owned()));
    assert!(matches!(address.decode(&[]), Err(ReadError::Abi(_))));

    for (read, answer, value) in [
        (
            win_fee_rate().unwrap(),
            "00000000000000000000000000000000000000000000000000470de4df820000",
            Ok(crate::trade::math::WIN_FEE_RATE),
        ),
        (
            pending_reward(wallet).unwrap(),
            "00000000000000000000000000000000000000000000003b4db9925a92b4c16e",
            Ok(0x3b_4db9_925a_92b4_c16e),
        ),
        (pending_reward(wallet).unwrap(), "0000000000000000000000000000000000000000000000000000000000000000", Ok(0)),
        (pending_reward(wallet).unwrap(), PAST_U128, Err(ReadError::Shape)),
        (win_fee_rate().unwrap(), PAST_U128, Err(ReadError::Shape)),
    ] {
        assert_eq!(read.decode(&word(answer)), value, "{answer}");
    }
    assert!(matches!(win_fee_rate().unwrap().decode(&[0; 31]), Err(ReadError::Abi(_))));
}

/// The reads added for the rates of a close, calldata from `cast calldata`, answers from the
/// mainnet RPC on 2026-11-24 (`fixtures/hyperevm/session_key.json`, `builder_fee_rate_of.json`,
/// `off_hours.json`).
#[test]
fn the_reads_for_a_close_s_rates_go_to_their_contracts() {
    let wallet: Address = WALLET.parse().unwrap();
    let key: Address = "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23".parse().unwrap();
    for ((to, data), contract, calldata, function) in [
        (
            parts(&session_key(wallet, key).unwrap()),
            SESSION_KEY_MANAGER,
            "0x96ade1f90000000000000000000000009587c978341b4de61fa14e4705f65fbaaec727410000000000000000000000002c7536e3605d9c16a7a3d7b1898e529396a65c23",
            "sessionKeys",
        ),
        (
            parts(&builder_fee_rate_of(wallet).unwrap()),
            BUILDER_REGISTRY,
            "0xd779e2700000000000000000000000009587c978341b4de61fa14e4705f65fbaaec72741",
            "feeRateOf",
        ),
        (parts(&off_hours().unwrap()), EXCHANGE, "0xf009ac03", "offHoursPayout"),
    ] {
        assert_eq!((to, hex::encode_prefixed(&data)), (contract, calldata.to_owned()), "{function}");
    }
}

fn recorded(name: &str) -> Vec<u8> {
    let text = match name {
        "session_key" => include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/hyperevm/session_key.json")),
        "builder_fee_rate_of" => {
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/hyperevm/builder_fee_rate_of.json"))
        }
        _ => include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/hyperevm/off_hours.json")),
    };
    let json: serde_json::Value = serde_json::from_str(text).unwrap();
    hex::decode(json["result"].as_str().unwrap()).unwrap()
}

#[test]
fn a_session_key_s_expiry_and_builder_decode() {
    let any: Address = WALLET.parse().unwrap();
    let read = session_key(any, any).unwrap();
    // a key of the recorded busy wallet: registered until 2026-12-02 and naming no builder
    assert_eq!(read.decode(&recorded("session_key")), Ok(RegisteredKey { expiry: 1_796_290_091, builder: None }));
    let builder = "6169d2c383451347b98faadec6f635aa59d8e4c4";
    let named = word(&format!("{:064x}{:0>64}", 1_796_290_091u64, builder));
    assert_eq!(
        read.decode(&named),
        Ok(RegisteredKey { expiry: 1_796_290_091, builder: Some(format!("0x{builder}").parse().unwrap()) })
    );
    assert_eq!(read.decode(&[0; 64]), Ok(RegisteredKey { expiry: 0, builder: None }), "never registered");
    assert!(matches!(read.decode(&[0; 32]), Err(ReadError::Abi(_))));
}

#[test]
fn a_builder_s_fee_and_the_off_hours_flag_decode() {
    let any: Address = WALLET.parse().unwrap();
    // trademarkets, approved on 2026-11-24
    let rate = builder_fee_rate_of(any).unwrap();
    assert_eq!(rate.decode(&recorded("builder_fee_rate_of")), Ok(crate::trade::math::BUILDER_FEE_RATE));
    assert_eq!(rate.decode(&[0; 32]), Ok(0), "not approved");
    assert_eq!(off_hours().unwrap().decode(&recorded("off_hours")), Ok(false));
    assert_eq!(off_hours().unwrap().decode(&word(&format!("{:064x}", 1))), Ok(true));
}
