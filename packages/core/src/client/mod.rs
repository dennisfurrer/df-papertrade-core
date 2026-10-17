//! The relayer, the deployment and the HyperEVM reads, and the [`Client`] an app trades through.

pub mod chain;
pub mod deployment;
pub mod error;
pub mod relayer;

pub use error::Error;
pub use relayer::{Accepted, Call, Relayer, RelayerError};

use df_hyperliquid_core::utils::Address;

use crate::markets::{QueueRank, QueueSummary, Summary, Trade, TradingState, stats};
use df_hyperliquid_core::client::evm::Rpc;
use df_hyperliquid_core::client::{Http, Network};

/// The venue endpoints, plus the clock and randomness the client uses.
#[derive(Clone, Debug)]
pub struct Config {
    /// Relayer base URL, or a same-origin proxy (the relayer refuses other origins).
    pub relayer: String,
    /// HyperEVM JSON-RPC URL.
    pub rpc: String,
    /// Hyperliquid `/exchange` URL, where deposit sends are posted.
    pub exchange: String,
    /// Wall clock in ms since the epoch, for deadlines, expiries and Hyperliquid nonces.
    pub now_ms: fn() -> u64,
    /// Fills the buffer from a CSPRNG, for session keys and first nonces.
    pub random: fn(&mut [u8]) -> Result<(), String>,
}

impl Config {
    /// The venue's mainnet hosts, with the caller's clock and randomness.
    #[must_use]
    pub fn mainnet(now_ms: fn() -> u64, random: fn(&mut [u8]) -> Result<(), String>) -> Config {
        Config {
            relayer: crate::client::deployment::ORIGIN.to_owned(),
            rpc: Network::Mainnet.evm_rpc_url().to_owned(),
            exchange: format!("{}/exchange", Network::Mainnet.api_url()),
            now_ms,
            random,
        }
    }

    /// Reaches the relayer through `relayer` (a proxy) instead.
    #[must_use]
    pub fn relayer(mut self, relayer: impl Into<String>) -> Config {
        self.relayer = relayer.into();
        self
    }
}

/// Hyperliquid's best bid and ask for a market and their mid, raw at the market's price scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub bid: u128,
    pub ask: u128,
    /// `(bid + ask) / 2`, the price an open or close executes at.
    pub mid: u128,
}

/// Papertrade client over one transport. Cheap to clone when the transport is.
#[derive(Clone, Debug)]
pub struct Client<H> {
    http: H,
    config: Config,
}

impl<H: Http + Clone> Client<H> {
    pub fn new(http: H, config: Config) -> Self {
        Client { http, config }
    }

    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        &self.http
    }

    pub(crate) fn now_ms(&self) -> u64 {
        (self.config.now_ms)()
    }

    pub(crate) fn random<const N: usize>(&self) -> Result<[u8; N], Error> {
        let mut bytes = [0u8; N];
        (self.config.random)(&mut bytes).map_err(Error::Random)?;
        Ok(bytes)
    }

    #[must_use]
    pub fn relayer(&self) -> Relayer<H> {
        Relayer::at(self.http.clone(), self.config.relayer.clone())
    }

    fn rpc(&self) -> Rpc<H> {
        Rpc::new(self.http.clone(), self.config.rpc.clone())
    }

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
        let bbo = self.rpc().read(&chain::bbo(asset_index)).await.map_err(Error::Precompile)?;
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
    /// Each `wallet` event's data folds into the account with `crate::account::Account::apply`.
    #[must_use]
    pub fn stream_url(&self, owner: Address) -> String {
        Call::account_stream(owner).request(&self.config.relayer).url
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
