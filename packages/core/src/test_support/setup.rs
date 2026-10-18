//! A client over the scripted wire with a fixed clock and randomness, and the store, wallet,
//! session key and account it drives.

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::utils::{Address, hex};

use super::{NOW_MS, SESSION_SECRET, WALLET_KEY, Wire, block_on};
use crate::account::store::{MemoryStore, Slot, Store};
use crate::account::wallet::LocalWallet;
use crate::account::{Account, SessionRecord};
use crate::client::{Client, Config};
use crate::utils::Lane;

pub(crate) const TTL: u64 = crate::account::session_key::SESSION_TTL_MS;
pub(crate) const WAD: u128 = crate::utils::units::WAD;
/// The app's proxy to the relayer, as a page reaches it.
pub(crate) const RELAY: &str = "/api/papertrade";
/// Mainnet's HyperEVM JSON-RPC, where the client's chain reads go.
pub(crate) const RPC: &str = "https://rpc.hyperliquid.xyz/evm";

pub(crate) fn now() -> u64 {
    NOW_MS
}

/// Fills with sevens: a valid secret and a known first nonce.
pub(crate) fn sevens(bytes: &mut [u8]) -> Result<(), String> {
    bytes.fill(7);
    Ok(())
}

pub(crate) fn config() -> Config {
    Config::mainnet(now, sevens).relayer(RELAY)
}

pub(crate) fn client(wire: &Wire) -> Client<Wire> {
    Client::new(wire.clone(), config())
}

pub(crate) fn wallet() -> LocalWallet {
    LocalWallet(LocalKey::from_hex(WALLET_KEY).unwrap())
}

pub(crate) fn owner() -> Address {
    wallet().0.address()
}

pub(crate) fn session_secret() -> [u8; 32] {
    hex::decode_array(SESSION_SECRET).unwrap()
}

pub(crate) fn session_address() -> Address {
    LocalKey::from_bytes(&session_secret()).unwrap().address()
}

pub(crate) fn record(registered: bool) -> SessionRecord {
    SessionRecord {
        secret: zeroize::Zeroizing::new(session_secret()),
        held_until_ms: NOW_MS + TTL,
        registered_at_ms: registered.then_some(NOW_MS),
    }
}

pub(crate) fn store(record: Option<&SessionRecord>, exchange: u128, staking: u128) -> MemoryStore {
    let store = MemoryStore::default();
    if let Some(record) = record {
        block_on(store.put(owner(), Slot::Session, record.to_bytes().to_vec(), None)).unwrap();
    }
    block_on(store.put(owner(), Slot::Nonce(Lane::Exchange), exchange.to_be_bytes().to_vec(), None)).unwrap();
    block_on(store.put(owner(), Slot::Nonce(Lane::Staking), staking.to_be_bytes().to_vec(), None)).unwrap();
    store
}

/// An account snapshot with 100 USDC available and nothing queued. `session_keys` holds JSON
/// `[key, expiry]` pairs.
pub(crate) fn snapshot(revision: u64, session_keys: &str, deposited: &str) -> String {
    format!(
        r#"{{"address":"{owner}","accountRevision":{revision},"indexedThroughBlock":100,"walletRevision":100,
        "indexedThroughTimestamp":{ts},"sessionKeys":[{session_keys}],"queueRevision":"1",
        "balance":"100000000000000000000","queued":"0","locked":"0",
        "funds":{{"reservedBalance":"0","reservedQueued":"0","availableBalance":"100000000000000000000","availableQueued":"0"}},
        "queueNodeCount":0,"consumableDebt":"0",
        "paper":{{"balance":"0","staked":"0","pendingBase":"0","rewardDebt":"0"}},
        "lifetime":{{"depositedRaw":"{deposited}","withdrawnRaw":"0","realizedPnlRaw":"0","feesPaidRaw":"0","tradedNotionalRaw":"0","paperRewardsClaimedRaw":"0"}},
        "operations":[],"openPositions":[]}}"#,
        owner = owner().to_hex(),
        ts = NOW_MS / 1000,
    )
}

pub(crate) fn account(session_keys: &str) -> Account {
    Account::apply(None, &snapshot(5, session_keys, "100000000000000000000"), owner()).unwrap().0
}

pub(crate) fn accepted(id: [u8; 32], action: &str) -> String {
    format!(
        r#"{{"intentId":"{}","action":"{action}","state":"queued","queueRevision":"2"}}"#,
        hex::encode_prefixed(&id)
    )
}
