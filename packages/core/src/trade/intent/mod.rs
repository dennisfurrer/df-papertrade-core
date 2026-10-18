//! Intents signed by a wallet or its session key. Each builds its EIP-712 struct, domain,
//! digest and wallet typed data, and once signed, the relayer payload and intent id.
//! The session key signs opens, closes and cancels. The wallet signs everything else.

pub mod cancel;
pub mod close;
pub mod open;
pub mod register;
pub mod signed;
pub mod staking;
pub mod typed;
pub mod withdraw;

pub use cancel::CancelIntent;
pub use close::{Close, MAX_CLOSE_IDS};
pub use open::{MAX_LEVERAGE, OpenPosition};
pub use register::RegisterSessionKey;
pub use signed::{BadSignature, Signed, intent_id, intent_id_tag};
pub use staking::{Claim, Staking};
pub use typed::Intent;
pub use withdraw::{HYPERCORE_ROUTE, MIN_WITHDRAWAL_USDC, WithdrawToCore};

/// How long an intent stays valid after it is made, in seconds.
pub const INTENT_TTL_SECS: u64 = 3600;

/// Deadline of an intent made at `now_ms`: one hour later, in Unix seconds.
#[must_use]
pub const fn deadline(now_ms: u64) -> u64 {
    now_ms / 1000 + INTENT_TTL_SECS
}

/// Which key signs an intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// The wallet itself, through `eth_signTypedData_v4`.
    Wallet,
    /// The wallet's registered session key.
    Session,
}

/// Why an intent was not built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("leverage {0} is outside 1 to {MAX_LEVERAGE}")]
    Leverage(u32),
    #[error("a position's margin must be more than zero")]
    NoMargin,
    #[error("a close names 1 to {MAX_CLOSE_IDS} positions, not {0}")]
    CloseCount(usize),
    #[error("a close names a position twice")]
    DuplicatePosition,
    #[error("a withdrawal moves at least 10 USDC")]
    WithdrawalTooSmall,
    #[error("the amount must be more than zero")]
    NoAmount,
    #[error("a session key is a fresh address, not the wallet or zero")]
    BadSessionKey,
    #[error("the session key expires before the registration does")]
    ExpiresFirst,
}

#[cfg(test)]
#[path = "tests/vectors.rs"]
pub(crate) mod vectors;

#[cfg(test)]
#[path = "tests/intent.rs"]
mod tests;
