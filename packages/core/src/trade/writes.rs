//! Intents posted to the relayer once each: open, close and cancel signed by the session key,
//! withdraw, stake, unstake and claim signed by the wallet. If the relayer does not answer, the
//! intent's result is unknown, not failed, and the account stream reports it by id.

use df_hyperliquid_core::utils::Address;

use crate::account::Account;
use crate::client::relayer::Accepted;
use crate::trade::{CancelIntent, Claim, Close, Intent, OpenPosition, Staking, WithdrawToCore, deadline, math};
use crate::utils::Lane;
use df_hyperliquid_core::client::Http;

use crate::account::store::Store;
use crate::account::wallet::{Purpose, Wallet};
use crate::client::{Client, Error};

/// A new position to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub instrument_id: u32,
    pub is_long: bool,
    /// Margin in collateral units (18 decimals).
    pub margin: u128,
    /// Leverage multiple, at most the market's cap.
    pub leverage: u32,
    /// Takes the margin from the queued balance instead of the available one.
    pub use_debt: bool,
}

impl<H: Http + Clone> Client<H> {
    /// Opens a position for `owner`, signed by the session key, at the BBO mid of the relayer's
    /// batch. It is first checked against a fresh trading state, `account` as last streamed, and
    /// `mark` (the current mid, raw) for the side's open-interest cap.
    pub async fn open(
        &self,
        store: &impl Store,
        owner: Address,
        account: &Account,
        open: Open,
        mark: Option<u128>,
    ) -> Result<Accepted, Error> {
        let state = self.trading_state().await?;
        let instrument = state
            .instrument(open.instrument_id)
            .ok_or_else(|| Error::Invalid(format!("no market {}", open.instrument_id)))?;
        let (available, room) = if open.use_debt {
            let room = math::debt_room(account.consumable_debt, account.queued, account.funds.available_queued);
            (account.funds.available_queued, Some(room))
        } else {
            (account.funds.available_balance, None)
        };
        math::check_open(&state, instrument, open.is_long, open.margin, open.leverage, mark, available, room)?;
        let key = self.session_key(store, owner, Some(account)).await?;
        let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
        let intent = OpenPosition::new(
            owner,
            open.instrument_id,
            open.is_long,
            open.margin,
            open.leverage,
            open.use_debt,
            nonce,
            deadline(self.now_ms()),
        )?;
        let signed =
            key.sign(Intent::OpenPosition(intent), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
        self.submit(&signed).await
    }

    /// Closes whole positions by id, at most 12 per intent, with one answer per intent. A failure
    /// after the first leaves the rest unsent.
    pub async fn close(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
        ids: &[u128],
    ) -> Result<Vec<Accepted>, Error> {
        if ids.is_empty() {
            return Err(Error::Invalid("no positions to close".into()));
        }
        let key = self.session_key(store, owner, account).await?;
        let mut answers = Vec::new();
        for chunk in ids.chunks(crate::trade::intent::MAX_CLOSE_IDS) {
            let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
            let close = Close::new(owner, chunk, nonce, deadline(self.now_ms()))?;
            let signed = key.sign(Intent::Close(close), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
            answers.push(self.submit(&signed).await?);
        }
        Ok(answers)
    }

    /// Cancels a queued intent (`intent_id`) before the relayer sends it. The relayer's answer is
    /// returned as is, without checking that it kept the cancel.
    pub async fn cancel(
        &self,
        store: &impl Store,
        owner: Address,
        account: Option<&Account>,
        intent_id: [u8; 32],
    ) -> Result<Accepted, Error> {
        let key = self.session_key(store, owner, account).await?;
        let cancel = CancelIntent { user: owner, intent_id, deadline: deadline(self.now_ms()) };
        let signed =
            key.sign(Intent::CancelIntent(cancel), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
        Ok(self.relayer().submit(&signed).await?)
    }

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
        let nonce = self.take_nonce(store, owner, Lane::Staking).await?;
        Ok(Staking::new(owner, amount, nonce, deadline(self.now_ms()))?)
    }
}
