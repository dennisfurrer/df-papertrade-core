//! Opening a position: checked against a fresh trading state and the account, then signed by
//! the session key.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::Address;

use crate::account::Account;
use crate::account::store::Store;
use crate::client::{Accepted, Client, Error};
use crate::trade::{Intent, OpenPosition, deadline, math};
use crate::utils::Lane;

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
    /// batch. It is first checked against a fresh trading state, `account` as last streamed (its
    /// funds and its open interest cap on the market), and `mark` (the current mid, raw) for the
    /// side's open-interest cap.
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
        let account_room = account.oi_room(&state, instrument.id);
        math::check_open(
            &state,
            instrument,
            open.is_long,
            open.margin,
            open.leverage,
            mark,
            available,
            room,
            account_room,
        )?;
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
}
