//! Registering a session key: the wallet authorizes a fresh key to sign for it until an expiry.

use df_hyperliquid_core::utils::Address;

use super::Refusal;

/// Authorizes a session key for the wallet until `expiry` (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterSessionKey {
    pub user: Address,
    pub key: Address,
    pub expiry: u64,
    /// Always the zero address.
    pub fee_address: Address,
    pub nonce: u128,
    pub deadline: u64,
}

impl RegisterSessionKey {
    /// Refuses a key that is zero or the wallet, or that expires before `deadline`.
    pub fn new(user: Address, key: Address, expiry: u64, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if key == user || key.is_zero() {
            return Err(Refusal::BadSessionKey);
        }
        if expiry <= deadline {
            return Err(Refusal::ExpiresFirst);
        }
        Ok(RegisterSessionKey { user, key, expiry, fee_address: Address::ZERO, nonce, deadline })
    }
}
