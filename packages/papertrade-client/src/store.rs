//! Where the client keeps what it holds per wallet between runs: the session
//! key ([`Slot::Session`], see [`crate::SessionRecord`]) and the next nonce
//! of each lane ([`Slot::Nonce`], 16 bytes big-endian). The caller says
//! where: a browser's credential store, a file, a keychain. The slot names
//! are what the venue's frontends already keep, so a key made by one is
//! found by the next build of it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;

use hl_core::Address;
use hl_papertrade::Lane;

/// One thing kept for a wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Session,
    Nonce(Lane),
}

impl Slot {
    /// The slot's name, as the frontends key it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Slot::Session => "session",
            Slot::Nonce(Lane::Exchange) => "nonce-exchange",
            Slot::Nonce(Lane::Staking) => "nonce-staking",
        }
    }
}

/// The store failed, in its words. Never quotes what was kept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the key store: {0}")]
pub struct StoreError(pub String);

/// Bytes kept per wallet and slot. No `Send` bound: the browser is
/// single-threaded.
pub trait Store {
    fn get(&self, wallet: Address, slot: Slot) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>>;

    /// Keeps `bytes` for `wallet` in `slot`; `ttl_ms`, when given, is how long
    /// they are worth keeping (a session key past its end is worthless).
    fn put(
        &self,
        wallet: Address,
        slot: Slot,
        bytes: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> impl Future<Output = Result<(), StoreError>>;
}

/// A store in memory, for tests and for a process that keeps nothing.
#[derive(Debug, Default)]
pub struct MemoryStore {
    kept: RefCell<HashMap<(Address, &'static str), Vec<u8>>>,
}

impl Store for MemoryStore {
    fn get(&self, wallet: Address, slot: Slot) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>> {
        std::future::ready(Ok(self.kept.borrow().get(&(wallet, slot.name())).cloned()))
    }

    fn put(
        &self,
        wallet: Address,
        slot: Slot,
        bytes: Vec<u8>,
        _ttl_ms: Option<u64>,
    ) -> impl Future<Output = Result<(), StoreError>> {
        self.kept.borrow_mut().insert((wallet, slot.name()), bytes);
        std::future::ready(Ok(()))
    }
}
