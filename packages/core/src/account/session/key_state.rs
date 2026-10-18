//! Whether the stored session key can sign, from its record, the account as last streamed and
//! the clock.

use super::SessionRecord;
use crate::account::Account;
use crate::account::session_key::{self, RENEW_BEFORE_MS, Status};

/// How long the indexer may lag behind an accepted registration before the key counts as not
/// registered, in ms.
pub const INDEXER_GRACE_MS: u64 = 120_000;

/// Whether the stored key can sign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// No key, or too near its end to sign. Make one.
    Missing,
    /// Stored but not registered, or its registration did not land. The wallet registers it.
    Unregistered,
    /// Signs now.
    Ready,
}

/// The key's state at `now_ms`. Under [`RENEW_BEFORE_MS`] left (about an hour, the life of a
/// signed intent) is [`KeyState::Missing`]. Otherwise the account's key list decides, or the
/// record when there is no account or the indexer is under [`INDEXER_GRACE_MS`] past registration.
#[must_use]
pub fn key_state(record: Option<&SessionRecord>, account: Option<&Account>, now_ms: u64) -> KeyState {
    let Some(record) = record else {
        return KeyState::Missing;
    };
    if record.held_until_ms <= now_ms.saturating_add(RENEW_BEFORE_MS) {
        return KeyState::Missing;
    }
    let Some(account) = account else {
        return if record.registered_at_ms.is_some() { KeyState::Ready } else { KeyState::Unregistered };
    };
    let Ok(address) = record.address() else {
        return KeyState::Missing;
    };
    match session_key::status(
        address,
        record.held_until_ms,
        &account.session_keys,
        account.indexed_through_timestamp,
        now_ms,
    ) {
        Status::Ready { .. } => KeyState::Ready,
        // accepted by the relayer, not yet indexed
        Status::Unregistered
            if record.registered_at_ms.is_some_and(|at| {
                account.indexed_through_timestamp.saturating_mul(1000) < at.saturating_add(INDEXER_GRACE_MS)
            }) =>
        {
            KeyState::Ready
        }
        Status::Unregistered | Status::Expiring { .. } => KeyState::Unregistered,
    }
}

#[cfg(test)]
#[path = "tests/key_state.rs"]
mod tests;
