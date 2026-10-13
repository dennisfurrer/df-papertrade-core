//! Papertrade for an app: what `hl-papertrade` defines (the intents, the
//! relayer, the account stream, the venue's math), put to work the way the
//! venue's own app works it, so every frontend trades it the same way.
//!
//! Owns:
//! - [`Client`]: the reads, from the relayer and HyperEVM, over the caller's
//!   [`hl_core::Http`];
//! - [`session`]: the session key's life: made from the caller's randomness,
//!   kept in the caller's [`Store`], registered once by the [`Wallet`],
//!   renewed when it nears its end, and whether it signs now
//!   ([`KeyState`]), from the store and the account stream;
//! - [`writes`]: open (checked first as the app checks it), close, cancel,
//!   signed by the session key; withdraw, stake, unstake and claim, signed by
//!   the wallet; each posted to the relayer once, never again;
//! - [`deposit`]: USDC from the wallet's Hyperliquid balance to its deposit
//!   address, as the app sends it.
//!
//! The caller brings the wire ([`hl_core::Http`]), the clock and the
//! randomness ([`Config`]), where the session key and the nonces are kept
//! ([`Store`]), and the wallet ([`Wallet`]); and it streams the account
//! itself (`hl_papertrade::sse`, `hl_papertrade::Account`), handing the
//! account as last folded to what needs it. No runtime. Mainnet only, as the
//! venue is. Money is integers, never floats.
#![deny(clippy::float_arithmetic)]

pub mod deposit;
pub mod error;
pub mod session;
pub mod store;
pub mod wallet;
pub mod writes;

pub use deposit::{ACTIVATION_FEE_USDC, Deposit, MIN_DEPOSIT_USDC, Mode, Send, Source};
pub use error::Error;
pub use hl_papertrade as protocol;
pub use session::{INDEXER_GRACE_MS, KeyState, SessionRecord, key_state};
pub use store::{MemoryStore, Slot, Store, StoreError};
pub use wallet::{LocalWallet, Purpose, SignRequest, Wallet, WalletError};
pub use writes::Open;

use hl_core::{Address, Http, Network};
use hl_evm::Rpc;
use hl_papertrade::relayer::{Call, Relayer};
use hl_papertrade::{QueueRank, QueueSummary, Summary, Trade, TradingState, chain, stats};

/// Where the client reaches the venue, and the clock and randomness it uses.
#[derive(Clone, Debug)]
pub struct Config {
    /// The relayer's root: its own host, or a proxy in front of it (it
    /// refuses other origins, so a page goes through its app).
    pub relayer: String,
    /// HyperEVM's JSON-RPC, for the reads made on chain.
    pub rpc: String,
    /// Hyperliquid's `/exchange`, where a deposit's sends go.
    pub exchange: String,
    /// The wall clock, milliseconds since the epoch: deadlines, expiries,
    /// Hyperliquid nonces.
    pub now_ms: fn() -> u64,
    /// Fills the buffer from a CSPRNG: fresh session keys and first nonces.
    pub random: fn(&mut [u8]) -> Result<(), String>,
}

impl Config {
    /// The venue's own hosts, with the caller's clock and randomness.
    #[must_use]
    pub fn mainnet(now_ms: fn() -> u64, random: fn(&mut [u8]) -> Result<(), String>) -> Config {
        Config {
            relayer: hl_papertrade::deployment::ORIGIN.to_owned(),
            rpc: Network::Mainnet.evm_rpc_url().to_owned(),
            exchange: format!("{}/exchange", Network::Mainnet.api_url()),
            now_ms,
            random,
        }
    }

    /// The same, with the relayer reached through `relayer` (a proxy).
    #[must_use]
    pub fn relayer(mut self, relayer: impl Into<String>) -> Config {
        self.relayer = relayer.into();
        self
    }
}

/// Hyperliquid's best bid and ask for a market, raw at its price scale, and
/// their mid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quote {
    pub bid: u128,
    pub ask: u128,
    /// `(bid + ask) / 2`, the price an open or close executes at.
    pub mid: u128,
}

/// Papertrade on one wire. Cheap to clone when the wire is.
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

    /// Every market, its parameters, open interest and caps, and what the
    /// relayer takes now.
    pub async fn trading_state(&self) -> Result<TradingState, Error> {
        Ok(TradingState::parse(&self.text(&Call::trading_state()).await?)?)
    }

    /// TVL, LP, queue, fees, PAPER's supply and curve.
    pub async fn summary(&self) -> Result<Summary, Error> {
        Ok(Summary::parse(&self.text(&Call::summary()).await?)?)
    }

    /// Each market's notional over the last day, by instrument id.
    pub async fn volumes(&self) -> Result<Vec<(u32, u128)>, Error> {
        Ok(stats::market_volumes(&self.text(&Call::market_stats()).await?)?)
    }

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

    /// The oracle's source for a market (`asset_index`, Hyperliquid's perp
    /// index), read on chain: the price an intent sent now executes at, give
    /// or take the block.
    pub async fn quote(&self, asset_index: u32) -> Result<Quote, Error> {
        let bbo = self.rpc().read(&chain::bbo(asset_index)).await.map_err(Error::Precompile)?;
        let (bid, ask) = (u128::from(bbo.bid), u128::from(bbo.ask));
        let mid = hl_papertrade::units::mid(bid, ask).ok_or(Error::Decode("the book is empty or crossed".into()))?;
        Ok(Quote { bid, ask, mid })
    }

    /// `owner`'s deposit address, derived on chain: whatever USDC reaches it
    /// on HyperCore is swept into the venue and credited to `owner`.
    pub async fn deposit_address(&self, owner: Address) -> Result<Address, Error> {
        let read = chain::deposit_address(owner).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// USDC rewards PaperStaking has distributed to `owner` and not yet paid,
    /// collateral units.
    pub async fn pending_reward(&self, owner: Address) -> Result<u128, Error> {
        let read = chain::pending_reward(owner).map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The Exchange's win fee now, an 18-decimal fraction.
    pub async fn win_fee_rate(&self) -> Result<u128, Error> {
        let read = chain::win_fee_rate().map_err(|e| Error::Decode(e.to_string()))?;
        read.run(&self.rpc()).await.map_err(Error::Chain)
    }

    /// The URL of `owner`'s account stream (server-sent events: `liveness`,
    /// `wallet`, `heartbeat`). Every `wallet` event's data folds into the
    /// account with `hl_papertrade::Account::apply`.
    #[must_use]
    pub fn stream_url(&self, owner: Address) -> String {
        Call::account_stream(owner).request(&self.config.relayer).url
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
