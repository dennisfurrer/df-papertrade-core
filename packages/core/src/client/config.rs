//! What a [`Client`](crate::Client) is set up with: the relayer, HyperEVM and Hyperliquid
//! endpoints, and the clock and randomness the caller supplies.

use df_hyperliquid_core::client::Network;
use df_hyperliquid_core::utils::Address;

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
    /// The builder code session keys name: the frontend's address that earns the builder fee on
    /// their closes. Zero for none. Named only while the BuilderRegistry approves it.
    pub builder: Address,
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
            builder: Address::ZERO,
        }
    }

    /// Names `builder` as the builder code in every session key registration, while approved.
    #[must_use]
    pub const fn builder(mut self, builder: Address) -> Config {
        self.builder = builder;
        self
    }

    /// Reaches the relayer through `relayer` (a proxy) instead.
    #[must_use]
    pub fn relayer(mut self, relayer: impl Into<String>) -> Config {
        self.relayer = relayer.into();
        self
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
