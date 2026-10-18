//! The session key as it is stored: the secret and its times in 48 bytes, never printed.

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::utils::Address;
use zeroize::Zeroizing;

use crate::account::session_key::SessionKey;
use crate::client::Error;

/// A stored session key. Stored as 48 bytes: the 32-byte secret, then `held_until_ms` and
/// `registered_at_ms` as big-endian u64 (0 when not registered).
#[derive(Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub secret: Zeroizing<[u8; 32]>,
    /// When the key stops being used, in ms since the epoch.
    pub held_until_ms: u64,
    /// When the relayer accepted its registration, in ms since the epoch.
    pub registered_at_ms: Option<u64>,
}

impl std::fmt::Debug for SessionRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRecord")
            .field("held_until_ms", &self.held_until_ms)
            .field("registered_at_ms", &self.registered_at_ms)
            .finish_non_exhaustive()
    }
}

impl SessionRecord {
    /// The 48-byte stored form.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(self.secret.to_vec());
        bytes.extend_from_slice(&self.held_until_ms.to_be_bytes());
        bytes.extend_from_slice(&self.registered_at_ms.unwrap_or(0).to_be_bytes());
        bytes
    }

    /// Parses what [`SessionRecord::to_bytes`] wrote, or `None` for anything else.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<SessionRecord> {
        if bytes.len() != 48 {
            return None;
        }
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(&bytes[..32]);
        let word = |at: usize| {
            let mut eight = [0u8; 8];
            eight.copy_from_slice(&bytes[at..at + 8]);
            u64::from_be_bytes(eight)
        };
        let registered = word(40);
        Some(SessionRecord {
            secret,
            held_until_ms: word(32),
            registered_at_ms: (registered > 0).then_some(registered),
        })
    }

    /// The key's address.
    pub fn address(&self) -> Result<Address, Error> {
        LocalKey::from_bytes(&self.secret)
            .map(|key| key.address())
            .map_err(|e| Error::Signature(format!("the kept session key: {e}")))
    }

    /// The key as a signer for `wallet`.
    pub fn signer(&self, wallet: Address) -> Result<SessionKey, Error> {
        SessionKey::from_secret(&self.secret, wallet, self.held_until_ms)
            .map_err(|e| Error::Signature(format!("the kept session key: {e}")))
    }
}

#[cfg(test)]
#[path = "tests/record.rs"]
mod tests;
