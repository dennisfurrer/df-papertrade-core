//! What the account sends, each intent signed and posted to the relayer once:
//!
//! - by the session key, no wallet prompt: [`Client::open`], [`Client::close`],
//!   [`Client::cancel`];
//! - by the wallet itself: [`Client::withdraw`] (to its own HyperCore spot
//!   account, never anywhere else), [`Client::stake`], [`Client::unstake`],
//!   [`Client::claim`].
//!
//! An open is checked first as the venue's app checks it (the switches, the
//! minimums, the market's size and leverage, the funds, the side's
//! open-interest cap) against the trading state, read fresh, and the account
//! as last streamed. A relayer that does not answer leaves the intent
//! unknown, not failed: the account stream's operations and outcomes name it
//! by id.

use hl_core::{Address, Http};
use hl_papertrade::relayer::Accepted;
use hl_papertrade::{
    Account, CancelIntent, Claim, Close, Intent, Lane, OpenPosition, Staking, WithdrawToCore, deadline, math,
};

use crate::store::Store;
use crate::wallet::{Purpose, Wallet};
use crate::{Client, Error};

/// A new position, as a ticket states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Open {
    pub instrument_id: u32,
    pub is_long: bool,
    /// Margin, collateral units (18 decimals).
    pub margin: u128,
    pub leverage: u32,
    /// From the queued balance rather than the paid-out one.
    pub use_debt: bool,
}

impl<H: Http + Clone> Client<H> {
    /// Opens a position at the BBO mid the relayer's batch executes at, for
    /// `owner`, whose account as last streamed is `account`. Checked against
    /// the trading state and `mark` (the mid now, raw) for the side's cap,
    /// then signed by the session key.
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

    /// Closes whole positions by id, at most twelve to an intent as the
    /// venue's app sends them; one answer per intent sent. A failure after
    /// the first leaves the rest unsent.
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
        for chunk in ids.chunks(hl_papertrade::intent::MAX_CLOSE_IDS) {
            let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
            let close = Close::new(owner, chunk, nonce, deadline(self.now_ms()))?;
            let signed = key.sign(Intent::Close(close), self.now_ms()).map_err(|e| Error::Signature(e.to_string()))?;
            answers.push(self.submit(&signed).await?);
        }
        Ok(answers)
    }

    /// Withdraws a queued intent (`intent_id`) before the relayer sends it.
    /// Its answer is taken as it comes, as the app takes it.
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

    /// Withdraws `usdc` (6 decimals, at least 10 USDC) of the available
    /// balance to the wallet's own HyperCore spot account. The wallet signs.
    pub async fn withdraw(
        &self,
        store: &impl Store,
        wallet: &impl Wallet,
        account: Option<&Account>,
        usdc: u128,
    ) -> Result<Accepted, Error> {
        let owner = wallet.address();
        if let Some(account) = account
            && usdc.saturating_mul(hl_papertrade::units::COLLATERAL_PER_USDC) > account.funds.available_balance
        {
            return Err(Error::InsufficientFunds("more than the available balance".into()));
        }
        let nonce = self.take_nonce(store, owner, Lane::Exchange).await?;
        let intent = WithdrawToCore::new(owner, usdc, nonce, deadline(self.now_ms()))?;
        let signed = self.wallet_sign(wallet, owner, Intent::WithdrawToCore(intent), Purpose::Withdraw).await?;
        self.submit(&signed).await
    }

    /// Stakes `amount` PAPER (18 decimals) at once; the wallet signs.
    pub async fn stake(&self, store: &impl Store, wallet: &impl Wallet, amount: u128) -> Result<Accepted, Error> {
        let intent = self.staking(store, wallet.address(), amount).await?;
        let signed = self.wallet_sign(wallet, wallet.address(), Intent::Stake(intent), Purpose::Stake).await?;
        self.submit(&signed).await
    }

    /// Unstakes `amount` PAPER at once; rewards already distributed stay
    /// claimable.
    pub async fn unstake(&self, store: &impl Store, wallet: &impl Wallet, amount: u128) -> Result<Accepted, Error> {
        let intent = self.staking(store, wallet.address(), amount).await?;
        let signed = self.wallet_sign(wallet, wallet.address(), Intent::Unstake(intent), Purpose::Unstake).await?;
        self.submit(&signed).await
    }

    /// Claims every distributed USDC reward into the account's balance.
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
