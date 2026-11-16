//! Session keys, registered once by the wallet so opens, closes and cancels need no wallet
//! prompt. A session key cannot withdraw, stake, unstake or claim.

pub mod signer;
pub mod status;

pub use signer::{SessionKey, SessionRefusal, may_sign};
pub use status::{Status, status};

use df_hyperliquid_core::utils::Address;

use crate::trade::intent::{Refusal, RegisterSessionKey, deadline};

/// How long a session key lasts: 30 days.
pub const SESSION_TTL_MS: u64 = 2_592_000_000;
/// Renew a key with less than this left: one intent lifetime (an hour) plus a minute.
pub const RENEW_BEFORE_MS: u64 = 3_660_000;

/// Expiry in Unix seconds of a key registered at `now_ms`.
#[must_use]
pub const fn expiry(now_ms: u64) -> u64 {
    (now_ms + SESSION_TTL_MS) / 1000
}

/// The registration the wallet signs for `key`, made at `now_ms` and held
/// until `held_until_ms`, naming `builder` (zero for none).
pub fn registration(
    user: Address,
    key: Address,
    held_until_ms: u64,
    nonce: u128,
    now_ms: u64,
    builder: Address,
) -> Result<RegisterSessionKey, Refusal> {
    Ok(RegisterSessionKey::new(user, key, held_until_ms / 1000, nonce, deadline(now_ms))?.builder(builder))
}

#[cfg(test)]
#[path = "tests/session_key.rs"]
mod tests;
