//! Where a held session key stands, against the account stream's registered keys and the clock.

use df_hyperliquid_core::utils::Address;

use super::RENEW_BEFORE_MS;

/// Where a held session key stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The account has not registered this key (or the indexer has not seen it).
    Unregistered,
    /// Registered, but too close to expiry to sign. Register a new one.
    Expiring { expires_at: u64 },
    /// Signs until `expires_at`, Unix seconds.
    Ready { expires_at: u64 },
}

/// Status of `key` held until `held_until_ms`, from the account stream's registered keys and
/// indexed time (both in seconds). Time is the later of `now_ms` and the indexed time.
#[must_use]
pub fn status(
    key: Address,
    held_until_ms: u64,
    registered: &[(Address, u64)],
    indexed_seconds: u64,
    now_ms: u64,
) -> Status {
    let Some((_, registered_expiry)) = registered.iter().find(|(address, _)| *address == key) else {
        return Status::Unregistered;
    };
    let expires_at = (*registered_expiry).min(held_until_ms / 1000);
    let now = now_ms.max(indexed_seconds.saturating_mul(1000));
    if expires_at.saturating_mul(1000) > now.saturating_add(RENEW_BEFORE_MS) {
        Status::Ready { expires_at }
    } else {
        Status::Expiring { expires_at }
    }
}

#[cfg(test)]
#[path = "tests/status.rs"]
mod tests;
