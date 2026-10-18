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
