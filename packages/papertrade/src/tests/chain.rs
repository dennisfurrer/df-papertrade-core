use hl_core::{Address, hex};

use super::*;

fn word(text: &str) -> Vec<u8> {
    hex::decode(text).unwrap()
}

/// Calldata against `cast calldata`, and answers as the mainnet RPC gave them
/// (2026-10-13).
#[test]
fn the_deposit_address_read_is_the_factorys() {
    let wallet: Address = "0x9587c978341b4de61fa14e4705f65fbaaec72741".parse().unwrap();
    let read = deposit_address(wallet).unwrap();
    assert_eq!(read.to, DEPOSIT_PROXY_FACTORY);
    assert_eq!(
        hex::encode_prefixed(&read.data),
        "0xd600539a0000000000000000000000009587c978341b4de61fa14e4705f65fbaaec72741"
    );
    let answer = word("000000000000000000000000c125c9a929bd75555a027dd563c89a116bd41c50");
    assert_eq!(read.decode(&answer).unwrap().to_hex(), "0xc125c9a929bd75555a027dd563c89a116bd41c50");
    assert!(matches!(read.decode(&[]), Err(ReadError::Abi(_))));
}

#[test]
fn the_fee_and_reward_reads_answer_integers() {
    let fee = win_fee_rate().unwrap();
    assert_eq!(fee.to, EXCHANGE);
    let answer = word("00000000000000000000000000000000000000000000000000470de4df820000");
    assert_eq!(fee.decode(&answer).unwrap(), crate::math::WIN_FEE_RATE);
    let wallet: Address = "0x9587c978341b4de61fa14e4705f65fbaaec72741".parse().unwrap();
    let pending = pending_reward(wallet).unwrap();
    assert_eq!(pending.to, PAPER_STAKING);
    assert_eq!(&pending.data[16..], wallet.bytes());
}

#[test]
fn the_bbo_reads_the_market_index() {
    let read = bbo(1);
    assert_eq!(read.at, hl_evm::precompiles::address(0x80e));
    assert_eq!(read.data, word("0000000000000000000000000000000000000000000000000000000000000001"));
}
