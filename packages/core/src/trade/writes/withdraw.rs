//! Withdrawing USDC to the wallet's own HyperCore spot account, signed by the wallet.

use df_hyperliquid_core::client::Http;

use crate::account::Account;
use crate::account::store::Store;
use crate::account::wallet::{Purpose, Wallet};
use crate::client::{Accepted, Client, Error};
use crate::trade::{Intent, WithdrawToCore, deadline};
use crate::utils::Lane;

impl<H: Http + Clone> Client<H> {
    /// Withdraws `usdc` (6 decimals, at least 10 USDC) of the available balance to the wallet's
    /// own HyperCore spot account, never anywhere else. The wallet signs.
    pub async fn withdraw(
        &self,
        store: &impl Store,
        wallet: &impl Wallet,
        account: Option<&Account>,
        usdc: u128,
    ) -> Result<Accepted, Error> {
        let owner = wallet.address();
        if let Some(account) = account
            && usdc.saturating_mul(crate::utils::units::COLLATERAL_PER_USDC) > account.funds.available_balance
        {
            return Err(Error::InsufficientFunds("more than the available balance".into()));
        }
        let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
        let intent = WithdrawToCore::new(owner, usdc, nonce, deadline(self.now_ms()))?;
        let signed = self.wallet_sign(wallet, owner, Intent::WithdrawToCore(intent), Purpose::Withdraw).await?;
        self.submit(&signed).await
    }
}

#[cfg(test)]
#[path = "tests/withdraw.rs"]
mod tests;
