//! Writes through the [`Client`](df_papertrade_core::Client): opens, closes and cancels signed by
//! the session key, withdrawals, stakes, unstakes and claims signed by the wallet, each posted once
//! to the relayer. Where one of viem's vectors (`vectors/papertrade_viem.json`) fits, the journey
//! replays it at its nonce, so the posted body is held byte for byte against what viem signed.
//! Every posted signature must recover to its signer over the intent the journey meant
//! ([`signed_by`]), and the relayer's answer must name that intent.

mod cancel;
mod close;
mod open;
mod rates;
mod refused;
mod staking;
mod withdraw;

use std::path::PathBuf;

use df_hyperliquid_core::account::{LocalKey, Signature};
use df_hyperliquid_core::utils::{Address, hex};
use df_papertrade_core::account::session_key::SESSION_TTL_MS;
use df_papertrade_core::account::{Account, MemoryStore, SessionRecord, Slot, Store};
use df_papertrade_core::trade::{Intent, Signed};
use df_papertrade_core::utils::Lane;
use df_papertrade_core::utils::units::WAD;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::support::{NOW, owner};

/// The vectors' session key, a published test key.
const SESSION_KEY: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";

/// BTC's mid at the recording, raw: $82,910.50.
pub const BTC_MARK: u128 = 829_105;

pub fn session_secret() -> [u8; 32] {
    hex::decode_array(SESSION_KEY).unwrap()
}

/// The session key's address.
pub fn session() -> Address {
    LocalKey::from_bytes(&session_secret()).unwrap().address()
}

/// A store with each lane's next nonce and no session key.
pub async fn lanes(exchange: u128, staking: u128) -> MemoryStore {
    let store = MemoryStore::default();
    for (lane, next) in [(Lane::Exchange, exchange), (Lane::Staking, staking)] {
        store.put(owner(), Slot::Nonce(lane), next.to_be_bytes().to_vec(), None).await.unwrap();
    }
    store
}

/// Keeps `record` as the owner's session key.
pub async fn keep(store: &MemoryStore, record: &SessionRecord) {
    store.put(owner(), Slot::Session, record.to_bytes().to_vec(), None).await.unwrap();
}

/// The session key as kept for 30 days from now, registered when `registered`.
pub fn record(registered: bool) -> SessionRecord {
    SessionRecord {
        secret: Zeroizing::new(session_secret()),
        held_until_ms: NOW + SESSION_TTL_MS,
        registered_at_ms: registered.then_some(NOW),
    }
}

/// A store whose session key was registered now, with each lane's next nonce.
pub async fn ready(exchange: u128, staking: u128) -> MemoryStore {
    let store = lanes(exchange, staking).await;
    keep(&store, &record(true)).await;
    store
}

/// The nonce `lane` hands out next, as the store keeps it.
pub async fn next(store: &MemoryStore, lane: Lane) -> u128 {
    let kept = store.get(owner(), Slot::Nonce(lane)).await.unwrap().unwrap();
    u128::from_be_bytes(kept.try_into().unwrap())
}

/// The owner's account as its stream's snapshot gives it: `balance` and `queued` dollars, all of
/// the queued balance consumable, nothing reserved or open, indexed through now.
pub fn account(balance: u128, queued: u128) -> Account {
    let (balance, queued) = ((balance * WAD).to_string(), (queued * WAD).to_string());
    let frame = json!({
        "address": owner().to_hex(),
        "accountRevision": 1,
        "indexedThroughBlock": 100,
        "walletRevision": 100,
        "indexedThroughTimestamp": NOW / 1000,
        "sessionKeys": [],
        "queueRevision": "1",
        "balance": balance,
        "queued": queued,
        "locked": "0",
        "funds": {"reservedBalance": "0", "reservedQueued": "0", "availableBalance": balance, "availableQueued": queued},
        "queueNodeCount": 0,
        "consumableDebt": queued,
        "paper": {"balance": "0", "staked": "0", "pendingBase": "0", "rewardDebt": "0"},
        "lifetime": {
            "depositedRaw": balance,
            "withdrawnRaw": "0",
            "realizedPnlRaw": "0",
            "feesPaidRaw": "0",
            "tradedNotionalRaw": "0",
            "paperRewardsClaimedRaw": "0",
        },
        "operations": [],
        "openPositions": [],
    });
    Account::apply(None, &frame.to_string(), owner()).unwrap().0
}

/// The viem vector named `name`.
pub fn vector(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vectors/papertrade_viem.json");
    let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let case = vectors["cases"].as_array().unwrap().iter().find(|case| case["name"] == name);
    case.unwrap_or_else(|| panic!("no vector named {name}")).clone()
}

/// A 32-byte id from its hex.
pub fn id(text: &str) -> [u8; 32] {
    hex::decode_array(text).unwrap()
}

/// What the relayer answers a submission it queued.
pub fn queued(intent_id: [u8; 32], action: &str) -> String {
    json!({"intentId": hex::encode_prefixed(&intent_id), "action": action, "state": "queued", "queueRevision": "882"})
        .to_string()
}

/// Holds a posted `body` to `intent` signed by `signer`: its signature recovers to `signer` over
/// the intent's digest, and the body is exactly what that signed intent posts, field for field.
pub fn signed_by(body: &Value, intent: Intent, signer: Address) -> Signed {
    let fields = body.get("signed").unwrap_or(body);
    let word = |key: &str| hex::decode_array(fields[key].as_str().unwrap()).unwrap();
    let v = fields["v"].as_str().unwrap().parse().unwrap();
    let signature = Signature::from_parts(word("r"), word("s"), v);
    let signed = Signed::recover(intent, signature, signer).unwrap_or_else(|error| panic!("{error}: {body}"));
    let expected: Value = serde_json::from_str(&signed.body()).unwrap();
    assert_eq!(*body, expected, "the body is the signed intent's");
    signed
}
