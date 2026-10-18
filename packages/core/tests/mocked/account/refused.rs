//! A registration that does not land leaves the key unregistered: the relayer refusing it or
//! keeping another, the wallet declining or forging, and no key made without randomness or a
//! store. Nothing is posted twice.

use std::future::{Future, ready};

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::utils::{Address, hex};
use df_papertrade_core::account::{KeyState, MemoryStore, Slot, Store, StoreError, Wallet, WalletError};
use df_papertrade_core::client::{Accepted, RelayerError};
use df_papertrade_core::utils::Lane;
use df_papertrade_core::{Client, Config, Error};

use super::{Declines, Forges, Prompted};
use crate::support::venue::Venue;
use crate::support::{client, clock, owner};
use crate::trade::{id, keep, lanes, next, queued, record, vector};

/// The intent id of the registration viem signed for the kept key.
fn registered() -> [u8; 32] {
    id(vector("register session key")["intentId"].as_str().unwrap())
}

/// The kept key, unregistered, with the exchange lane at the vector's nonce.
async fn unregistered() -> MemoryStore {
    let store = lanes(3_170_816, 1).await;
    keep(&store, &record(false)).await;
    store
}

#[tokio::test]
async fn a_registration_the_relayer_does_not_keep_leaves_the_key_unregistered() {
    let other = [0xab; 32];
    let rejected = format!(
        r#"{{"intentId":"{}","action":"registerSessionKey","state":"rejected"}}"#,
        hex::encode_prefixed(&registered())
    );
    let refused = |status, code: &str, message: Option<&str>| {
        Error::Relayer(RelayerError::Refused {
            status,
            code: Some(code.to_owned()),
            message: message.map(str::to_owned),
        })
    };
    let cases = [
        (
            "a refusal",
            Venue::new()
                .status("/intents", 400, r#"{"error":"nonce_used","message":"nonce already used"}"#)
                .post("/intents", "{}"),
            refused(400, "nonce_used", Some("nonce already used")),
        ),
        (
            "an answer that is not ok",
            Venue::new().post("/intents", r#"{"ok":false,"error":"intake_paused"}"#),
            refused(200, "intake_paused", None),
        ),
        (
            "another intent kept",
            Venue::new().post("/intents", &queued(other, "registerSessionKey")),
            Error::Relayer(RelayerError::OtherIntent {
                sent: hex::encode_prefixed(&registered()),
                found: hex::encode_prefixed(&other),
            }),
        ),
        (
            "an intent rejected",
            Venue::new().post("/intents", &rejected),
            Error::NotKept(Accepted {
                intent_id: registered(),
                action: "registerSessionKey".to_owned(),
                state: "rejected".to_owned(),
                queue_revision: None,
            }),
        ),
    ];
    for (name, venue, expected) in cases {
        let store = unregistered().await;
        let client = client(&venue);
        assert_eq!(client.enable(&store, &Prompted::new(), None).await, Err(expected), "{name}");
        assert_eq!(venue.relayed(), ["POST /intents"], "{name}: posted once");
        assert_eq!(client.session(&store, owner()).await.unwrap(), Some(record(false)), "{name}");
        assert_eq!(client.key_state(&store, owner(), None).await.unwrap(), KeyState::Unregistered, "{name}");
        assert_eq!(next(&store, Lane::Exchange).await, 3_170_817, "{name}: the nonce is spent all the same");
    }
}

#[tokio::test]
async fn an_answer_that_does_not_read_is_no_registration() {
    let venue = Venue::new().post("/intents", "Bad Gateway");
    let store = unregistered().await;
    let got = client(&venue).enable(&store, &Prompted::new(), None).await;
    assert!(matches!(got, Err(Error::Relayer(RelayerError::Unreadable(_)))), "{got:?}");
    assert_eq!(client(&venue).session(&store, owner()).await.unwrap(), Some(record(false)));
}

/// Enables the kept key with `wallet`: what came back, and what the venue took.
async fn enable_with(wallet: &impl Wallet) -> (Result<KeyState, Error>, Venue, MemoryStore) {
    let venue = Venue::new();
    let store = unregistered().await;
    let got = client(&venue).enable(&store, wallet, None).await;
    (got, venue, store)
}

#[tokio::test]
async fn a_wallet_that_declines_or_forges_registers_nothing() {
    let forger = LocalKey::from_bytes(&[9; 32]).unwrap().address();
    let cases = [
        ("declines", enable_with(&Declines).await, Error::Wallet(WalletError::Declined)),
        ("forges", enable_with(&Forges).await, Error::Signature(format!("signed by {forger}, not {}", owner()))),
    ];
    for (name, (got, venue, store), expected) in cases {
        assert_eq!(got, Err(expected), "{name}");
        assert!(venue.sent().is_empty(), "{name}: nothing posted");
        assert_eq!(client(&venue).session(&store, owner()).await.unwrap(), Some(record(false)), "{name}");
    }
}

/// A store that cannot be reached.
struct Locked;

impl Store for Locked {
    fn get(&self, _: Address, _: Slot) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>> {
        ready(Err(StoreError("locked".to_owned())))
    }

    fn put(&self, _: Address, _: Slot, _: Vec<u8>, _: Option<u64>) -> impl Future<Output = Result<(), StoreError>> {
        ready(Err(StoreError("locked".to_owned())))
    }
}

#[tokio::test]
async fn no_key_is_made_without_randomness_or_a_store() {
    let venue = Venue::new();
    let without = |random| Client::new(venue.clone(), Config::mainnet(clock, random));
    let store = MemoryStore::default();
    let cases = [
        (
            "no randomness",
            without(|_| Err("no entropy".to_owned())).new_session(&store, owner()).await.map(drop),
            Error::Random("no entropy".to_owned()),
        ),
        (
            "a secret that is no key",
            without(|bytes| {
                bytes.fill(0);
                Ok(())
            })
            .new_session(&store, owner())
            .await
            .map(drop),
            Error::Signature("the kept session key: the key is not a valid secp256k1 private key".to_owned()),
        ),
        (
            "no store",
            client(&venue).new_session(&Locked, owner()).await.map(drop),
            Error::Store(StoreError("locked".to_owned())),
        ),
    ];
    for (name, got, expected) in cases {
        assert_eq!(got, Err(expected), "{name}");
    }
    assert_eq!(client(&venue).session(&store, owner()).await.unwrap(), None, "nothing kept");
    assert!(venue.sent().is_empty());
}
