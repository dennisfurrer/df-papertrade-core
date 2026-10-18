//! What every write shares: the owner's wallet signs, and each lane counts up on its own.

use df_hyperliquid_core::account::LocalKey;

use super::*;
use crate::account::store::MemoryStore;
use crate::account::wallet::LocalWallet;
use crate::test_support::{NOW_MS, Wire, block_on, client, owner};
use crate::trade::{Claim, deadline};

#[test]
fn another_wallet_cannot_sign_for_the_account() {
    let stranger = LocalWallet(LocalKey::from_bytes(&[9u8; 32]).unwrap());
    let wire = Wire::default();
    let intent = Intent::Claim(Claim { user: owner(), nonce: 1, deadline: deadline(NOW_MS) });
    let e = block_on(client(&wire).wallet_sign(&stranger, owner(), intent, Purpose::Claim)).unwrap_err();
    assert!(matches!(e, Error::WrongWallet { .. }));
    assert!(wire.sent().is_empty());
}

#[test]
fn nonces_count_up_from_a_random_start() {
    let wire = Wire::default();
    let client = client(&wire);
    let store = MemoryStore::default();
    let first = block_on(client.take_nonce(&store, owner(), Lane::Exchange)).unwrap();
    assert_eq!(first, nonce::first([7, 7, 7]));
    assert_eq!(block_on(client.take_nonce(&store, owner(), Lane::Exchange)).unwrap(), first + 1);
    assert_eq!(block_on(client.take_nonce(&store, owner(), Lane::Staking)).unwrap(), first, "its own lane");
}
