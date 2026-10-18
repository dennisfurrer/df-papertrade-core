//! The account and who signs for it.
//!
//! - [`state`]: the live account folded from the relayer's stream.
//! - [`session_key`]: the key that signs opens, closes and cancels, and its registration.
//! - [`session`]: the stored key's record, whether it can sign, and the client's lifecycle for it.
//! - [`store`]: where the session key and nonces are kept between runs.
//! - [`wallet`]: the account's own wallet, for everything a session key may not sign.

pub mod session;
pub mod session_key;
pub mod state;
pub mod store;
pub mod wallet;

pub use session::{INDEXER_GRACE_MS, KeyState, SessionRecord, key_state};
pub use session_key::{SessionKey, SessionRefusal, Status};
pub use state::{Account, AccountError, Applied, ClosedPosition, Operation, OperationKind, Outcome, Position};
pub use store::{MemoryStore, Slot, Store, StoreError};
pub use wallet::{LocalWallet, Purpose, SignRequest, Wallet, WalletError};
