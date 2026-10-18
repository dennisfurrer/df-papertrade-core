//! A session key from nothing to its first open, a kept key registered as viem signs it, a key
//! renewed near its end, and the store keeping it for the rest of its life.

use std::cell::RefCell;
use std::future::Future;

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::utils::Address;
use df_hyperliquid_core::utils::sign::digest_of_wallet_json;
use df_papertrade_core::Error;
use df_papertrade_core::account::session_key::{RENEW_BEFORE_MS, SESSION_TTL_MS};
use df_papertrade_core::account::{KeyState, MemoryStore, Purpose, SessionRecord, Slot, Store, StoreError};
use df_papertrade_core::trade::{Intent, Open, OpenPosition, RegisterSessionKey, deadline, intent_id};
use df_papertrade_core::utils::units::WAD;
use df_papertrade_core::utils::{Lane, nonce};
use zeroize::Zeroizing;

use super::Prompted;
use crate::support::venue::Venue;
use crate::support::{NOW, client, owner};
use crate::trade::{BTC_MARK, account, id, keep, lanes, next, queued, record, signed_by, vector};

/// The key `sevens` makes.
fn sevens_key() -> Address {
    LocalKey::from_bytes(&[7; 32]).unwrap().address()
}

/// The registration the wallet signs for `key`, made now at `nonce`.
fn registration(key: Address, nonce: u128) -> Intent {
    let expiry = (NOW + SESSION_TTL_MS) / 1000;
    Intent::RegisterSessionKey(RegisterSessionKey::new(owner(), key, expiry, nonce, deadline(NOW)).unwrap())
}

#[tokio::test]
async fn a_first_session_is_registered_by_the_wallet_then_signs_an_open() {
    let key = sevens_key();
    let first = nonce::first([7, 7, 7]);
    let register = registration(key, first);
    let open = OpenPosition::new(owner(), 0, true, 10 * WAD, 1000, false, first + 1, deadline(NOW)).unwrap();
    let open = Intent::OpenPosition(open);
    let (registered, opened) = (intent_id(&register.digest(), owner()), intent_id(&open.digest(), key));
    let venue = Venue::new()
        .post("/intents", &queued(registered, "registerSessionKey"))
        .get("/state/trading", "trading.json")
        .post("/intents", &queued(opened, "openPosition"));
    let client = client(&venue);
    let (store, wallet) = (MemoryStore::default(), Prompted::new());

    let made = client.new_session(&store, owner()).await.unwrap();
    assert_eq!((*made.secret, made.held_until_ms, made.registered_at_ms), ([7; 32], NOW + SESSION_TTL_MS, None));
    assert_eq!(made.address().unwrap(), key);
    assert_eq!(client.new_session(&store, owner()).await.unwrap(), made, "kept, not made again");
    assert_eq!(client.key_state(&store, owner(), None).await.unwrap(), KeyState::Unregistered);
    assert_eq!(client.session_key(&store, owner(), None).await.unwrap_err(), Error::NeedsKey(KeyState::Unregistered));
    assert!(venue.sent().is_empty(), "making a key asks nothing of the relayer");

    assert_eq!(client.enable(&store, &wallet, None).await.unwrap(), KeyState::Ready);
    assert_eq!(client.enable(&store, &wallet, None).await.unwrap(), KeyState::Ready, "a ready key asks nothing");
    let asked = wallet.asked();
    let [prompt] = asked.as_slice() else { panic!("one wallet prompt, not {}", asked.len()) };
    assert_eq!((prompt.purpose, prompt.digest), (Purpose::RegisterSessionKey, register.digest()));
    assert_eq!(digest_of_wallet_json(&prompt.json).unwrap(), register.digest(), "the typed data is what it signs");
    let kept = client.session(&store, owner()).await.unwrap().unwrap();
    assert_eq!(kept, SessionRecord { registered_at_ms: Some(NOW), ..made });

    let ticket = Open { instrument_id: 0, is_long: true, margin: 10 * WAD, leverage: 1000, use_debt: false };
    let answer = client.open(&store, owner(), &account(100, 0), ticket, Some(BTC_MARK)).await.unwrap();

    assert_eq!(venue.relayed(), ["POST /intents", "GET /state/trading", "POST /intents"]);
    let posted = venue.posted("/intents");
    assert_eq!(signed_by(&posted[0], register, owner()).intent_id(), registered, "the wallet signed the registration");
    assert_eq!(signed_by(&posted[1], open, key).intent_id(), opened, "the new key signed the open");
    assert_eq!((answer.intent_id, answer.state.as_str()), (opened, "queued"));
    assert_eq!(next(&store, Lane::Exchange).await, first + 2);
}

#[tokio::test]
async fn a_kept_key_is_registered_as_viem_signs_it() {
    let case = vector("register session key");
    let store = lanes(3_170_816, 1).await;
    keep(&store, &record(false)).await;
    let venue = Venue::new().post("/intents", &queued(id(case["intentId"].as_str().unwrap()), "registerSessionKey"));
    let client = client(&venue);

    assert_eq!(client.enable(&store, &Prompted::new(), None).await.unwrap(), KeyState::Ready);

    let posted = venue.posted("/intents");
    assert_eq!(posted, [serde_json::json!({"action": "registerSessionKey", "signed": case["signed"]})]);
    let signed = signed_by(&posted[0], registration(crate::trade::session(), 3_170_816), owner());
    assert_eq!(hex(&signed.intent_id()), case["intentId"]);
    assert_eq!(client.session(&store, owner()).await.unwrap(), Some(record(true)));
    assert_eq!((next(&store, Lane::Exchange).await, next(&store, Lane::Staking).await), (3_170_817, 1));
}

#[tokio::test]
async fn a_key_near_its_end_is_made_again_before_it_is_registered() {
    let store = lanes(3_170_816, 1).await;
    let ending = SessionRecord {
        secret: Zeroizing::new([9; 32]),
        held_until_ms: NOW + RENEW_BEFORE_MS,
        registered_at_ms: Some(NOW - SESSION_TTL_MS),
    };
    keep(&store, &ending).await;
    let register = registration(sevens_key(), 3_170_816);
    let venue = Venue::new().post("/intents", &queued(intent_id(&register.digest(), owner()), "registerSessionKey"));
    let client = client(&venue);

    assert_eq!(client.key_state(&store, owner(), None).await.unwrap(), KeyState::Missing);
    assert_eq!(client.enable(&store, &Prompted::new(), None).await.unwrap(), KeyState::Ready);

    signed_by(&venue.posted("/intents")[0], register, owner());
    let renewed = client.session(&store, owner()).await.unwrap().unwrap();
    assert_eq!(
        (*renewed.secret, renewed.held_until_ms, renewed.registered_at_ms),
        ([7; 32], NOW + SESSION_TTL_MS, Some(NOW))
    );
    assert_eq!(client.session_key(&store, owner(), None).await.unwrap().address(), sevens_key());
}

/// A store that keeps what it is given, and how long each was meant to stay.
#[derive(Default)]
struct Timed {
    store: MemoryStore,
    puts: RefCell<Vec<(&'static str, Option<u64>)>>,
}

impl Store for Timed {
    fn get(&self, wallet: Address, slot: Slot) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>> {
        self.store.get(wallet, slot)
    }

    fn put(
        &self,
        wallet: Address,
        slot: Slot,
        bytes: Vec<u8>,
        ttl: Option<u64>,
    ) -> impl Future<Output = Result<(), StoreError>> {
        self.puts.borrow_mut().push((slot.name(), ttl));
        self.store.put(wallet, slot, bytes, ttl)
    }
}

#[tokio::test]
async fn the_key_is_kept_for_the_rest_of_its_life_and_a_nonce_for_good() {
    let register = registration(sevens_key(), nonce::first([7, 7, 7]));
    let venue = Venue::new().post("/intents", &queued(intent_id(&register.digest(), owner()), "registerSessionKey"));
    let store = Timed::default();

    client(&venue).enable(&store, &Prompted::new(), None).await.unwrap();

    let life = Some(SESSION_TTL_MS);
    assert_eq!(store.puts.take(), [("session", life), ("nonce-exchange", None), ("session", life)]);
}

fn hex(bytes: &[u8; 32]) -> String {
    df_hyperliquid_core::utils::hex::encode_prefixed(bytes)
}
