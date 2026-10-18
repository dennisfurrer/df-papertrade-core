//! What the [`Client`] reads: the relayer's trading state and protocol figures, the book's BBO
//! and the contracts' values on HyperEVM, and where the account stream is.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::client::evm::precompiles;
use df_hyperliquid_core::utils::Address;

use crate::client::{Call, Client, Error, chain};
use crate::markets::{QueueRank, QueueSummary, Summary, Trade, TradingState, stats};

/// Hyperliquid's best bid and ask for a market and their mid, raw at the market's price scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub bid: u128,
    pub ask: u128,
    /// `(bid + ask) / 2`, the price an open or close executes at.
    pub mid: u128,
}

impl<H: Http + Clone> Client<H> {
    async fn text(&self, call: &Call) -> Result<String, Error> {
        Ok(self.relayer().send(call).await?)
    }

    /// Every market's parameters, open interest and caps, and which intents the relayer
    /// accepts now.
    pub async fn trading_state(&self) -> Result<TradingState, Error> {
        Ok(TradingState::parse(&self.text(&Call::trading_state()).await?)?)
    }

    /// TVL, LP, queue, fees, and PAPER's supply and curve.
    pub async fn summary(&self) -> Result<Summary, Error> {
        Ok(Summary::parse(&self.text(&Call::summary()).await?)?)
    }

    /// Each market's notional over the last day, by instrument id.
    pub async fn volumes(&self) -> Result<Vec<(u32, u128)>, Error> {
        Ok(stats::market_volumes(&self.text(&Call::market_stats()).await?)?)
    }

    /// The payout queue: total owed to queued winners, count, and its largest and front entries.
    pub async fn queue(&self) -> Result<QueueSummary, Error> {
        Ok(QueueSummary::parse(&self.text(&Call::queue_summary()).await?)?)
    }

    /// The latest trades across the venue, newest first.
    pub async fn recent_trades(&self) -> Result<Vec<Trade>, Error> {
        Ok(stats::recent_trades(&self.text(&Call::recent_trades()).await?)?)
    }

    /// Where `owner`'s queued intents stand in the relayer's queue.
    pub async fn queue_rank(&self, owner: Address) -> Result<QueueRank, Error> {
        Ok(QueueRank::parse(&self.text(&Call::queue_rank(owner)).await?)?)
    }

    /// The market's BBO from the oracle's on-chain source (`asset_index` is Hyperliquid's perp
    /// index). An intent sent now executes at its mid, give or take a block.
    pub async fn quote(&self, asset_index: u32) -> Result<Quote, Error> {
        let bbo = self.rpc().read(&precompiles::bbo(asset_index)).await.map_err(Error::Precompile)?;
        let (bid, ask) = (u128::from(bbo.bid), u128::from(bbo.ask));
        let mid = crate::utils::units::mid(bid, ask).ok_or(Error::Decode("the book is empty or crossed".into()))?;
        Ok(Quote { bid, ask, mid })
    }

    /// `owner`'s deposit address, derived on chain. USDC sent to it on HyperCore is swept into
    /// the venue and credited to `owner`.
    pub async fn deposit_address(&self, owner: Address) -> Result<Address, Error> {
        let read = chain::deposit_address(owner).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// USDC staking rewards distributed to `owner` and not yet paid, in collateral units.
    pub async fn pending_reward(&self, owner: Address) -> Result<u128, Error> {
        let read = chain::pending_reward(owner).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The Exchange contract's current win fee, an 18-decimal fraction.
    pub async fn win_fee_rate(&self) -> Result<u128, Error> {
        let read = chain::win_fee_rate().map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// URL of `owner`'s account stream (server-sent events `liveness`, `wallet`, `heartbeat`).
    /// Each `wallet` event's data folds into the account with
    /// [`Account::apply`](crate::account::Account::apply).
    #[must_use]
    pub fn stream_url(&self, owner: Address) -> String {
        Call::account_stream(owner).request(&self.config().relayer).url
    }
}

#[cfg(test)]
#[path = "tests/reads.rs"]
mod tests;
