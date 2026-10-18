//! One wallet's account from the relayer's public live stream (`GET /state/user/live?wallet=`).
//! A snapshot comes first, then patches against a named revision. [`Account::apply`] folds
//! each frame in, and [`AccountError::OutOfSync`] means the stream must be reopened.
//!
//! - [`position`]: an open position.
//! - [`operation`]: an intent the relayer accepted and has not settled.
//! - [`closed`]: positions a patch reports closed, and how intents ended.
//! - [`funds`]: reserved and available funds, PAPER and lifetime totals.
//!
//! Folding a frame in is split by step: the entry point and the checks every frame passes, a
//! snapshot, a patch, and the state fields both carry.

mod apply;
pub mod closed;
mod fields;
pub mod funds;
pub mod operation;
mod patch;
pub mod position;
mod snapshot;

pub use closed::{ClosedPosition, Outcome};
pub use funds::{Funds, Lifetime, Paper};
pub use operation::{Operation, OperationKind};
pub use position::Position;

use df_hyperliquid_core::utils::Address;

use crate::utils::wire::BadWire;

/// One wallet, as of its latest frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub address: Address,
    pub account_revision: u64,
    pub indexed_through_block: u64,
    pub wallet_revision: u64,
    /// Unix seconds of the last block the indexer read.
    pub indexed_through_timestamp: u64,
    /// Registered session keys and when each expires, Unix seconds.
    pub session_keys: Vec<(Address, u64)>,
    pub queue_revision: u128,
    /// Paid out, in collateral units.
    pub balance: u128,
    /// Owed by the payout queue, waiting on harvests.
    pub queued: u128,
    /// Margin in open positions.
    pub locked: u128,
    pub funds: Funds,
    pub queue_node_count: u64,
    /// The queued balance the next queue entries could fund as margin.
    pub consumable_debt: u128,
    pub paper: Paper,
    pub lifetime: Lifetime,
    pub operations: Vec<Operation>,
    /// Sorted by position id, newest first.
    pub positions: Vec<Position>,
    /// The closes and liquidations the latest patch reported.
    pub closed: Vec<ClosedPosition>,
    /// The intent outcomes the latest patch reported.
    pub outcomes: Vec<([u8; 32], Outcome)>,
}

/// Why a frame was not folded in.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccountError {
    #[error(transparent)]
    Wire(#[from] BadWire),
    /// The frame does not follow on or does not add up. Reopen the stream.
    #[error("the account stream is out of sync: {0}")]
    OutOfSync(&'static str),
    #[error("the frame is for {found}, not {expected}")]
    OtherWallet { expected: Address, found: Address },
}

/// What a frame did to the account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Snapshot,
    Patch,
    /// Older than the held account, so nothing changed.
    Stale,
}

impl Account {
    /// The registered expiry of `key`, Unix seconds.
    #[must_use]
    pub fn session_expiry(&self, key: Address) -> Option<u64> {
        self.session_keys.iter().find(|(address, _)| *address == key).map(|(_, expiry)| *expiry)
    }

    /// Whether the wallet has ever deposited.
    #[must_use]
    pub const fn has_deposited(&self) -> bool {
        self.lifetime.deposited > 0
    }
}
