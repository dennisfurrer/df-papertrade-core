//! Staking and unstaking PAPER, and claiming the USDC rewards staking distributes.

use df_hyperliquid_core::utils::Address;

use super::Refusal;

/// Stakes or unstakes `amount` PAPER (18 decimals). Both take effect at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staking {
    pub user: Address,
    pub amount: u128,
    pub nonce: u128,
    pub deadline: u64,
}

impl Staking {
    /// Refuses a zero amount.
    pub fn new(user: Address, amount: u128, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        if amount == 0 {
            return Err(Refusal::NoAmount);
        }
        Ok(Staking { user, amount, nonce, deadline })
    }
}

/// Claims every distributed USDC reward into the Exchange balance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub user: Address,
    pub nonce: u128,
    pub deadline: u64,
}
