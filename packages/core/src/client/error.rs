//! The client's error type. It separates refusals made before sending, steps to take first,
//! answers from the relayer or Hyperliquid, and failures of the store, wallet or transport.

use crate::client::relayer::{Accepted, RelayerError};
use crate::trade::Refusal;
use crate::trade::math::OpenRefusal;
use crate::utils::BadWire;

use crate::account::session::KeyState;
use crate::account::store::StoreError;
use crate::account::wallet::WalletError;

/// Why a client call failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The intent is malformed (leverage out of range, no margin, no positions to close).
    /// Nothing was sent.
    #[error("{0}")]
    Invalid(String),
    /// The protocol would refuse the open. Nothing was sent.
    #[error(transparent)]
    Open(OpenRefusal),
    /// More than the account holds. Nothing was sent.
    #[error("{0}")]
    InsufficientFunds(String),
    /// The account has not streamed yet, so nothing can be checked.
    #[error("the account has not streamed yet")]
    NoAccount,
    /// The session key cannot sign yet. [`KeyState::Missing`] needs a key made,
    /// [`KeyState::Unregistered`] needs the wallet to register it.
    #[error("{}", match .0 {
        KeyState::Missing => "enable trading first: no session key",
        KeyState::Unregistered => "register the session key first",
        KeyState::Ready => "the session key is ready",
    })]
    NeedsKey(KeyState),
    /// The signing wallet is not the account's owner.
    #[error("the wallet is {wallet}, not the account's owner {owner}")]
    WrongWallet { wallet: String, owner: String },
    #[error(transparent)]
    Wallet(WalletError),
    /// A signature that does not recover to the account's owner, or a session key that cannot
    /// sign.
    #[error("{0}")]
    Signature(String),
    #[error(transparent)]
    Relayer(#[from] RelayerError),
    /// The relayer answered but did not keep the intent.
    #[error("the relayer answered {}", .0.state)]
    NotKept(Accepted),
    /// Hyperliquid refused a deposit send, with its message. Later sends were not made.
    #[error("Hyperliquid refused the send: {0}")]
    Exchange(String),
    #[error(transparent)]
    Transport(#[from] df_hyperliquid_core::client::TransportError),
    #[error(transparent)]
    Chain(crate::client::chain::ReadError),
    #[error(transparent)]
    Precompile(df_hyperliquid_core::client::evm::ReadError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("no randomness: {0}")]
    Random(String),
    /// An answer that could not be decoded.
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
