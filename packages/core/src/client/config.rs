//! What a [`Client`](crate::Client) is set up with: the relayer, HyperEVM and Hyperliquid
//! endpoints, and the clock and randomness the caller supplies.

use df_hyperliquid_core::client::Network;

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

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
