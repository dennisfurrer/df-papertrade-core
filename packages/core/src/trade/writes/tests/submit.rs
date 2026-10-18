//! What every write shares: only the owner's wallet signs for the account, a signed intent is
//! posted once as it is, and each lane counts up on its own from a random start or where it was
//! kept. The first two are out of reach of the public writes, which always sign as the wallet
//! given and sign the intents they post.

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::client::Method;

use super::*;
use crate::account::store::MemoryStore;
use crate::account::wallet::LocalWallet;
use crate::test_support::{NOW_MS, Wire, accepted, block_on, client, owner, store, wallet};
use crate::trade::intent::vectors::{intent_of, signature_of, signer_of, text, vectors};
use crate::trade::{Claim, deadline};

#[test]
fn only_the_owners_wallet_signs_for_the_account() {
    let stranger = LocalWallet(LocalKey::from_bytes(&[9u8; 32]).unwrap());
    let claim = |user| Intent::Claim(Claim { user, nonce: 1, deadline: deadline(NOW_MS) });
    let wrong = |wallet: Address| Error::WrongWallet { wallet: wallet.to_hex(), owner: owner().to_hex() };
    let wire = Wire::default();
    let cases = [
        (
            "another wallet",
            block_on(client(&wire).wallet_sign(&stranger, owner(), claim(owner()), Purpose::Claim)),
            wrong(stranger.address()),
        ),
        (
            "an intent for another wallet",
            block_on(client(&wire).wallet_sign(&wallet(), owner(), claim(stranger.address()), Purpose::Claim)),
            wrong(stranger.address()),
        ),
    ];
    for (name, got, expected) in cases {
        assert_eq!(got.map(|signed| signed.signer()), Err(expected), "{name}");
    }
    assert!(wire.sent().is_empty());
    let signed = block_on(client(&wire).wallet_sign(&wallet(), owner(), claim(owner()), Purpose::Claim)).unwrap();
    assert_eq!(signed.signer(), owner());
}

#[test]
fn a_signed_intent_is_posted_once_as_it_is() {
    let vectors = vectors();
    let case = vectors["cases"].as_array().unwrap().iter().find(|case| text(case, "name") == "claim rewards").unwrap();
    let (_, signer) = signer_of(&vectors, case);
    let signed = intent_of(case).unwrap().signed(signature_of(case), signer);
    let answer = accepted(signed.intent_id(), "paperClaim");
    let wire = Wire::default().on(Method::Post, "/api/papertrade/intents", "", 200, &answer);
    assert_eq!(block_on(client(&wire).submit(&signed)).unwrap().intent_id, signed.intent_id());
    let posted = wire.sent();
    assert_eq!(posted.len(), 1);
    assert_eq!(posted[0].body, signed.body().into_bytes());
}

#[test]
fn each_lane_counts_up_from_its_own_random_start() {
    let client = client(&Wire::default());
    let store = MemoryStore::default();
    let first = nonce::first([7, 7, 7]);
    let takes = [
        (Lane::Exchange, first),
        (Lane::Exchange, first + 1),
        (Lane::Staking, first),
        (Lane::Exchange, first + 2),
        (Lane::Staking, first + 1),
    ];
    for (index, (lane, expected)) in takes.into_iter().enumerate() {
        assert_eq!(block_on(client.take_nonce(&store, owner(), lane)).unwrap(), expected, "take {index}, {lane:?}");
    }
}

#[test]
fn a_kept_lane_resumes_where_it_stopped() {
    let client = client(&Wire::default());
    let store = store(None, 40, 900);
    for (lane, expected) in [(Lane::Exchange, 40), (Lane::Staking, 900), (Lane::Exchange, 41), (Lane::Staking, 901)] {
        assert_eq!(block_on(client.take_nonce(&store, owner(), lane)).unwrap(), expected, "{lane:?}");
    }
}

#[test]
fn a_lane_kept_in_any_other_form_starts_again() {
    let client = client(&Wire::default());
    let store = MemoryStore::default();
    block_on(store.put(owner(), Slot::Nonce(Lane::Exchange), vec![1, 2, 3], None)).unwrap();
    assert_eq!(block_on(client.take_nonce(&store, owner(), Lane::Exchange)).unwrap(), nonce::first([7, 7, 7]));
    let kept = block_on(store.get(owner(), Slot::Nonce(Lane::Exchange))).unwrap().unwrap();
    assert_eq!(kept, (nonce::first([7, 7, 7]) + 1).to_be_bytes(), "kept as 16 bytes");
}
