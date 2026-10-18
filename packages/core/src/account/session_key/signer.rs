//! A session key held in memory, and the checks it makes before it signs an intent.

use df_hyperliquid_core::account::{LocalKey, SignError};
use df_hyperliquid_core::utils::Address;

use crate::trade::intent::{Intent, Key, Signed};

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
    #[error(transparent)]
    Curve(#[from] SignError),
}

/// The checks a session key makes before signing `intent` for `wallet`.
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

/// A session key for one wallet, held in memory. The caller persists the secret. It is
/// wiped on drop.
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

    /// When the key stops signing, Unix milliseconds.
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

#[cfg(test)]
#[path = "tests/signer.rs"]
mod tests;
