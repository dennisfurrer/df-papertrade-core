//! Session keys: a key the app holds, which the wallet registers once so that
//! opens, closes and cancels need no wallet prompt. It cannot withdraw, stake,
//! unstake or claim; the worst a stolen one does is trade the balance away.
//!
//! - **Lifetime**: 30 days from registration ([`SESSION_TTL_MS`]); the app
//!   registers a new one once less than [`RENEW_BEFORE_MS`] is left, since an
//!   intent signed now stays valid for an hour.
//! - **Status** ([`status`]): from the account stream's registered keys and
//!   the key the app holds, by the later of the clock and the indexer.
//! - **Signing** ([`SessionKey`], the `sign` feature): refuses what the app's
//!   own signer refuses: any intent a session key may not sign, another
//!   wallet's, an expired one, or with an expired key.

use hl_core::Address;

use crate::intent::{Intent, Key, Refusal, RegisterSessionKey, deadline};

/// How long a session key lasts: 30 days.
pub const SESSION_TTL_MS: u64 = 2_592_000_000;
/// Renew a key once it has less than this left: an hour, the life of an intent
/// it signs, and a minute.
pub const RENEW_BEFORE_MS: u64 = 3_660_000;

/// A key registered at `now_ms` expires at this, in Unix seconds.
#[must_use]
pub const fn expiry(now_ms: u64) -> u64 {
    (now_ms + SESSION_TTL_MS) / 1000
}

/// Where the app's key stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The account has not registered this key (or the indexer has not seen it).
    Unregistered,
    /// Registered, but with too little time left to sign: register a new one.
    Expiring { expires_at: u64 },
    /// Signs until `expires_at`, Unix seconds.
    Ready { expires_at: u64 },
}

/// The status of `key`, held until `held_until_ms`, given the keys the account
/// stream lists (`(key, expiry seconds)`), the stream's indexed time (seconds)
/// and the clock.
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

/// The registration the wallet signs for `key`, made at `now_ms` and held
/// until `held_until_ms`.
pub fn registration(
    user: Address,
    key: Address,
    held_until_ms: u64,
    nonce: u128,
    now_ms: u64,
) -> Result<RegisterSessionKey, Refusal> {
    RegisterSessionKey::new(user, key, held_until_ms / 1000, nonce, deadline(now_ms))
}

/// Why a session key would not sign.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionRefusal {
    #[error("a session key cannot sign {0}; the wallet signs it")]
    NotSessionIntent(&'static str),
    #[error("the intent acts for {intent}, not this session's wallet {wallet}")]
    OtherWallet { intent: Address, wallet: Address },
    #[error("the intent's deadline has passed")]
    Expired,
    #[error("the session key has expired")]
    KeyExpired,
    #[cfg(feature = "sign")]
    #[error(transparent)]
    Curve(#[from] hl_core::SignError),
}

/// The checks a session key makes before it signs `intent` for `wallet`.
pub fn may_sign(intent: &Intent, wallet: Address, held_until_ms: u64, now_ms: u64) -> Result<(), SessionRefusal> {
    if intent.key() != Key::Session {
        return Err(SessionRefusal::NotSessionIntent(intent.primary_type()));
    }
    if intent.user() != wallet {
        return Err(SessionRefusal::OtherWallet { intent: intent.user(), wallet });
    }
    if intent.deadline() <= now_ms / 1000 {
        return Err(SessionRefusal::Expired);
    }
    if held_until_ms <= now_ms {
        return Err(SessionRefusal::KeyExpired);
    }
    Ok(())
}

#[cfg(feature = "sign")]
pub use signing::SessionKey;

#[cfg(feature = "sign")]
mod signing {
    use hl_core::{Address, LocalKey, SignError};

    use super::{SessionRefusal, may_sign};
    use crate::intent::{Intent, Signed};

    /// The app's session key for one wallet, held in memory until it expires.
    /// Its secret is the caller's to keep (the app stores it per wallet and
    /// deployment); it is wiped when this is dropped.
    pub struct SessionKey {
        key: LocalKey,
        wallet: Address,
        held_until_ms: u64,
    }

    impl std::fmt::Debug for SessionKey {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SessionKey")
                .field("address", &self.key.address())
                .field("wallet", &self.wallet)
                .field("held_until_ms", &self.held_until_ms)
                .finish()
        }
    }

    impl SessionKey {
        /// From 32 secret bytes (fresh from a CSPRNG for a new key).
        pub fn from_secret(secret: &[u8; 32], wallet: Address, held_until_ms: u64) -> Result<SessionKey, SignError> {
            Ok(SessionKey { key: LocalKey::from_bytes(secret)?, wallet, held_until_ms })
        }

        #[must_use]
        pub const fn address(&self) -> Address {
            self.key.address()
        }

        #[must_use]
        pub const fn wallet(&self) -> Address {
            self.wallet
        }

        #[must_use]
        pub const fn held_until_ms(&self) -> u64 {
            self.held_until_ms
        }

        /// Signs `intent` if a session key may, for this wallet, now.
        pub fn sign(&self, intent: Intent, now_ms: u64) -> Result<Signed, SessionRefusal> {
            may_sign(&intent, self.wallet, self.held_until_ms, now_ms)?;
            let signature = self.key.sign_digest(&intent.digest())?;
            Ok(intent.signed(signature, self.key.address()))
        }
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
