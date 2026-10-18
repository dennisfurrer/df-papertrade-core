//! Staking and unstaking PAPER and claiming its rewards, signed by the wallet from the staking
//! lane's nonces.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;

use crate::account::store::Store;
use crate::account::wallet::{Purpose, Wallet};
use crate::client::{Accepted, Client, Error};
use crate::trade::{Claim, Intent, Staking, deadline};
use crate::utils::Lane;

impl<H: Http + Clone> Client<H> {
    /// Stakes `amount` PAPER (18 decimals) immediately. The wallet signs.
    pub async fn stake(&self, store: &impl Store, wallet: &impl Wallet, amount: u128) -> Result<Accepted, Error> {
        let intent = self.staking(store, wallet.address(), amount).await?;
        let signed = self.wallet_sign(wallet, wallet.address(), Intent::Stake(intent), Purpose::Stake).await?;
        self.submit(&signed).await
    }

    /// Unstakes `amount` PAPER (18 decimals) immediately. Distributed rewards stay claimable. The
    /// wallet signs.
    pub async fn unstake(&self, store: &impl Store, wallet: &impl Wallet, amount: u128) -> Result<Accepted, Error> {
        let intent = self.staking(store, wallet.address(), amount).await?;
        let signed = self.wallet_sign(wallet, wallet.address(), Intent::Unstake(intent), Purpose::Unstake).await?;
        self.submit(&signed).await
    }

    /// Claims every distributed USDC reward into the account's balance. The wallet signs.
    pub async fn claim(&self, store: &impl Store, wallet: &impl Wallet) -> Result<Accepted, Error> {
        let owner = wallet.address();
        let nonce = self.take_nonce(store, owner, Lane::Staking).await?;
        let claim = Claim { user: owner, nonce, deadline: deadline(self.now_ms()) };
        let signed = self.wallet_sign(wallet, owner, Intent::Claim(claim), Purpose::Claim).await?;
        self.submit(&signed).await
    }

    async fn staking(&self, store: &impl Store, owner: Address, amount: u128) -> Result<Staking, Error> {
        Staking::check(amount)?;
        let nonce = self.take_nonce(store, owner, Lane::Staking).await?;
        Ok(Staking::new(owner, amount, nonce, deadline(self.now_ms()))?)
    }
}
