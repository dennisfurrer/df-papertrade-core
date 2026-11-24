//! What the [`Client`] reads: the relayer's trading state and protocol figures, the book's BBO
//! and the contracts' values on HyperEVM, and where the account stream is.

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::client::evm::precompiles;
use df_hyperliquid_core::utils::Address;

use crate::client::{Call, Client, Error, chain};
use crate::markets::{Builder, Builders, QueueRank, QueueSummary, Summary, Trade, TradingState, stats};

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

    /// What `builder` has earned and not claimed, in collateral units.
    pub async fn builder_fees(&self, builder: Address) -> Result<u128, Error> {
        let read = chain::builder_fees(builder).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// Whether the BuilderRegistry approves `builder`, so a session key may name it.
    pub async fn is_builder(&self, builder: Address) -> Result<bool, Error> {
        let read = chain::is_builder(builder).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The builder fee on a close's PnL, an 18-decimal fraction.
    pub async fn builder_fee_rate(&self) -> Result<u128, Error> {
        let read = chain::builder_fee_rate().map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The builder fee a close through `builder` pays now, an 18-decimal fraction: the registry's
    /// rate while it approves `builder`, else 0.
    pub async fn builder_fee_of(&self, builder: Address) -> Result<u128, Error> {
        let read = chain::builder_fee_rate_of(builder).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// `key` as the SessionKeyManager holds it for `owner`: its expiry and the builder it names.
    pub async fn registered_key(&self, owner: Address, key: Address) -> Result<chain::RegisteredKey, Error> {
        let read = chain::session_key(owner, key).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The builder fee a close signed by `key` for `owner` pays now, an 18-decimal fraction, for
    /// [`Rates::current`](crate::trade::math::Rates::current): the fee rate of the builder the
    /// key's registration names while the registry approves it, else 0. One read when the key
    /// names no builder, two otherwise.
    pub async fn key_builder_fee(&self, owner: Address, key: Address) -> Result<u128, Error> {
        match self.registered_key(owner, key).await?.builder {
            Some(builder) => self.builder_fee_of(builder).await,
            None => Ok(0),
        }
    }

    /// Whether the off-hours keeper has switched winning closes to the off-hours payout curve, as
    /// the Exchange holds it now.
    pub async fn off_hours(&self) -> Result<bool, Error> {
        let read = chain::off_hours().map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// Every builder's routing and earnings, with the registry and its fee rate.
    pub async fn builders(&self) -> Result<Builders, Error> {
        Ok(Builders::parse(&self.text(&Call::builders()).await?)?)
    }

    /// One builder's standing: approved or not, unclaimed fees and what it routed.
    pub async fn builder(&self, builder: Address) -> Result<Builder, Error> {
        Ok(Builder::parse(&self.text(&Call::builder(builder)).await?)?)
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
