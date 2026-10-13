//! Why the client did not do what was asked, each kind as a frontend tells it
//! apart: refused before anything was sent, a step to take first, the
//! relayer's or Hyperliquid's answer, or the store, the wallet or the wire.

use hl_papertrade::math::OpenRefusal;
use hl_papertrade::relayer::{Accepted, RelayerError};
use hl_papertrade::{BadWire, Refusal};

use crate::session::KeyState;
use crate::store::StoreError;
use crate::wallet::WalletError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The intent is malformed (a leverage out of range, no margin, no
    /// positions to close). Nothing was sent.
    #[error("{0}")]
    Invalid(String),
    /// The protocol would refuse the open, as the app checks it. Nothing was
    /// sent.
    #[error(transparent)]
    Open(OpenRefusal),
    /// More than the account holds. Nothing was sent.
    #[error("{0}")]
    InsufficientFunds(String),
    /// The account has not streamed yet, so nothing can be checked.
    #[error("the account has not streamed yet")]
    NoAccount,
    /// The session key does not sign yet: [`KeyState::Missing`] wants one
    /// made, [`KeyState::Unregistered`] the wallet's registration.
    #[error("{}", match .0 {
        KeyState::Missing => "enable trading first: no session key",
        KeyState::Unregistered => "register the session key first",
        KeyState::Ready => "the session key is ready",
    })]
    NeedsKey(KeyState),
    /// The wallet signing is not the account's owner.
    #[error("the wallet is {wallet}, not the account's owner {owner}")]
    WrongWallet { wallet: String, owner: String },
    #[error(transparent)]
    Wallet(WalletError),
    /// A signature that does not recover to the account's owner, or a session
    /// key that would not sign.
    #[error("{0}")]
    Signature(String),
    #[error(transparent)]
    Relayer(#[from] RelayerError),
    /// The relayer answered, and did not keep the intent (its state).
    #[error("the relayer answered {}", .0.state)]
    NotKept(Accepted),
    /// Hyperliquid refused a deposit's send, in its words. Any after it were
    /// not sent.
    #[error("Hyperliquid refused the send: {0}")]
    Exchange(String),
    #[error(transparent)]
    Transport(#[from] hl_core::TransportError),
    #[error(transparent)]
    Chain(hl_papertrade::chain::ReadError),
    #[error(transparent)]
    Precompile(hl_evm::ReadError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("no randomness: {0}")]
    Random(String),
    /// An answer that did not read.
    #[error("{0}")]
    Decode(String),
}

impl From<BadWire> for Error {
    fn from(e: BadWire) -> Self {
        Error::Decode(e.to_string())
    }
}

impl From<Refusal> for Error {
    fn from(e: Refusal) -> Self {
        Error::Invalid(e.to_string())
    }
}

impl From<OpenRefusal> for Error {
    fn from(e: OpenRefusal) -> Self {
        Error::Open(e)
    }
}

impl From<WalletError> for Error {
    fn from(e: WalletError) -> Self {
        Error::Wallet(e)
    }
}
