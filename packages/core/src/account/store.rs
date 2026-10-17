//! Per-wallet storage that persists the session key and nonces between runs. The caller
//! implements [`Store`] over a browser credential store, a file or a keychain.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;

use crate::utils::Lane;
use df_hyperliquid_core::utils::Address;

/// One value kept per wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The session key, as a 48-byte [`SessionRecord`](crate::account::SessionRecord).
    Session,
    /// A lane's next nonce, 16 bytes big-endian.
    Nonce(Lane),
}

impl Slot {
    /// The slot's storage key. Stable, so existing stores stay readable.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Slot::Session => "session",
            Slot::Nonce(Lane::Exchange) => "nonce-exchange",
            Slot::Nonce(Lane::Staking) => "nonce-staking",
        }
    }
}

/// A store failure, in the store's words. Never contains the stored bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the key store: {0}")]
pub struct StoreError(pub String);

/// Bytes kept per wallet and slot. No `Send` bound, so it works in single-threaded browser wasm.
pub trait Store {
    /// The bytes kept for `wallet` in `slot`, if any.
    fn get(&self, wallet: Address, slot: Slot) -> impl Future<Output = Result<Option<Vec<u8>>, StoreError>>;

    /// Keeps `bytes` for `wallet` in `slot`. `ttl_ms`, when given, is how long they stay useful
    /// (a session key's remaining life).
    fn put(
        &self,
        wallet: Address,
        slot: Slot,
        bytes: Vec<u8>,
        ttl_ms: Option<u64>,
    ) -> impl Future<Output = Result<(), StoreError>>;
}

/// An in-memory store, for tests and processes that persist nothing. Ignores `ttl_ms`.
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
