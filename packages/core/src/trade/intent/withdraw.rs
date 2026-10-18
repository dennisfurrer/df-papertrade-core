//! Withdrawing USDC from the venue to the wallet's own HyperCore spot account, the only route
//! that needs no second signature.

use df_hyperliquid_core::utils::Address;

use super::Refusal;

/// Smallest withdrawal: 10 USDC, in 6 decimals.
pub const MIN_WITHDRAWAL_USDC: u128 = 10_000_000;
/// Relayer withdrawal route to the wallet's own HyperCore spot account. The only route that
/// needs no second signature.
pub const HYPERCORE_ROUTE: &str = "hypercore";

/// Moves `amount` USDC (6 decimals) of the available balance to the wallet's
/// HyperCore spot account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawToCore {
    pub user: Address,
    pub amount: u128,
    pub nonce: u128,
    pub deadline: u64,
}

impl WithdrawToCore {
    /// Refuses less than [`MIN_WITHDRAWAL_USDC`].
    pub fn new(user: Address, amount: u128, nonce: u128, deadline: u64) -> Result<Self, Refusal> {
        WithdrawToCore::check(amount)?;
        Ok(WithdrawToCore { user, amount, nonce, deadline })
    }

    /// What [`WithdrawToCore::new`] refuses, checked before a nonce is spent.
    pub const fn check(amount: u128) -> Result<(), Refusal> {
        if amount < MIN_WITHDRAWAL_USDC { Err(Refusal::WithdrawalTooSmall) } else { Ok(()) }
    }
}
