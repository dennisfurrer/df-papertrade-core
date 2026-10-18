//! The session key's lifecycle: made from the caller's randomness, kept in the
//! [`Store`](crate::account::Store) for 30 days, registered once by the wallet, and renewed near
//! its end. It signs opens, closes and cancels without a wallet prompt, and can never withdraw,
//! stake or claim.

pub mod key_state;
mod lifecycle;
pub mod record;

pub use key_state::{INDEXER_GRACE_MS, KeyState, key_state};
pub use record::SessionRecord;
