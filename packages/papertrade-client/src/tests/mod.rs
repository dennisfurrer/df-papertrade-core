//! The client on fixtures: what it reads, the session key's life, and what it
//! would send, byte for byte the intents hl-papertrade builds (whose bytes
//! are checked against viem there). The wallet is a key in memory signing the
//! digest it is handed; a scripted wire answers by method, URL and a piece of
//! the body, and keeps what was sent.

mod deposit;
mod reads;
mod session;
mod writes;

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use hl_core::{Address, Http, LocalKey, Method, Request, Response, TransportError, hex};
use hl_papertrade::{Account, Lane};

use crate::store::{MemoryStore, Slot, Store};
use crate::wallet::LocalWallet;
use crate::{Client, Config, SessionRecord};

pub(crate) const TRADING: &str = include_str!("../../../papertrade/fixtures/trading.json");
pub(crate) const NOW_MS: u64 = 1_791_860_400_000;
pub(crate) const WALLET_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
pub(crate) const SESSION_SECRET: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
pub(crate) const TTL: u64 = hl_papertrade::session::SESSION_TTL_MS;
pub(crate) const WAD: u128 = hl_papertrade::units::WAD;
pub(crate) const RELAY: &str = "/api/papertrade";

pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

pub(crate) fn now() -> u64 {
    NOW_MS
}

/// Not random: sevens, a valid secret and a known first nonce.
pub(crate) fn sevens(bytes: &mut [u8]) -> Result<(), String> {
    bytes.fill(7);
    Ok(())
}

pub(crate) fn config() -> Config {
    Config::mainnet(now, sevens).relayer(RELAY)
}

/// One scripted answer: method, URL prefix, a piece the body holds (any case), status, body.
pub(crate) type Route = (Method, &'static str, &'static str, u16, String);

/// Answers by method, URL prefix and a piece of the body: two JSON-RPC calls share a URL.
#[derive(Default)]
pub(crate) struct Scripted {
    routes: RefCell<Vec<Route>>,
    seen: RefCell<Vec<Request>>,
}

/// The scripted wire, shared with the client.
#[derive(Clone, Default)]
pub(crate) struct Wire(Rc<Scripted>);

impl Wire {
    pub(crate) fn on(self, method: Method, url: &'static str, piece: &'static str, status: u16, body: &str) -> Self {
        self.0.routes.borrow_mut().push((method, url, piece, status, body.to_owned()));
        self
    }

    pub(crate) fn sent(&self) -> Vec<Request> {
        self.0.seen.borrow().clone()
    }

    pub(crate) fn posts_to(&self, url: &str) -> Vec<Request> {
        self.sent().into_iter().filter(|r| r.method == Method::Post && r.url.starts_with(url)).collect()
    }
}

impl Http for Wire {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        let body = String::from_utf8_lossy(&request.body).to_lowercase();
        let found = self.0.routes.borrow().iter().find_map(|(method, url, piece, status, answer)| {
            (*method == request.method && request.url.starts_with(url) && body.contains(&piece.to_lowercase()))
                .then(|| Response { status: *status, headers: Vec::new(), body: answer.clone().into_bytes() })
        });
        let answer = found.ok_or_else(|| TransportError::Unreachable(format!("no fixture for {} {body}", request.url)));
        self.0.seen.borrow_mut().push(request);
        std::future::ready(answer)
    }
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

/// A kept session key, registered or not, held for its full 30 days.
pub(crate) fn record(registered: bool) -> SessionRecord {
    SessionRecord {
        secret: zeroize::Zeroizing::new(session_secret()),
        held_until_ms: NOW_MS + TTL,
        registered_at_ms: registered.then_some(NOW_MS),
    }
}

/// A store holding `record` and the lanes' next nonces.
pub(crate) fn store(record: Option<&SessionRecord>, exchange: u128, staking: u128) -> MemoryStore {
    let store = MemoryStore::default();
    if let Some(record) = record {
        block_on(store.put(owner(), Slot::Session, record.to_bytes().to_vec(), None)).unwrap();
    }
    block_on(store.put(owner(), Slot::Nonce(Lane::Exchange), exchange.to_be_bytes().to_vec(), None)).unwrap();
    block_on(store.put(owner(), Slot::Nonce(Lane::Staking), staking.to_be_bytes().to_vec(), None)).unwrap();
    store
}

/// A snapshot of the owner's account at `revision`: $100 available, nothing queued, the given
/// session keys (JSON `[key, expiry]` pairs) and lifetime deposits.
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

pub(crate) fn body(request: &Request) -> serde_json::Value {
    serde_json::from_slice(&request.body).unwrap()
}
