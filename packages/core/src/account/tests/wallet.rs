//! The local wallet: it signs a request's digest as its key, and prints its address, never its key.

use df_hyperliquid_core::account::recover;

use super::*;
use crate::test_support::{WALLET_KEY, block_on, owner, wallet};

#[test]
fn a_local_wallet_signs_the_digest_as_its_key() {
    for (purpose, digest) in [(Purpose::RegisterSessionKey, [7; 32]), (Purpose::Withdraw, [1; 32])] {
        let request = SignRequest { purpose, json: String::new(), digest };
        let signature = block_on(wallet().sign(&request)).unwrap();
        assert_eq!(recover(&digest, &signature).unwrap(), owner(), "{purpose:?}");
    }
}

#[test]
fn a_local_wallet_prints_its_address_never_its_key() {
    let printed = format!("{:?}", wallet());
    assert_eq!(printed, format!("LocalWallet({:?})", owner()));
    assert!(!printed.contains(&WALLET_KEY[2..12]));
}
