//! The [`Client`] an app trades through, and what it stands on.
//!
//! - [`config`]: the venue endpoints, the clock and randomness.
//! - [`relayer`]: relayer requests as values, their answers, and the sender.
//! - [`chain`]: contract reads on HyperEVM.
//! - [`deployment`]: contract addresses, the relayer origin and the EIP-712 domains.
//! - [`error`]: why a client call failed.
//!
//! The client's reads are here too. Its writes live with their domains: the session key in
//! [`crate::account::session`], trades and staking in [`crate::trade::writes`], deposits in
//! [`crate::transfer::deposit`].

pub mod chain;
pub mod config;
pub mod deployment;
pub mod error;
mod reads;
pub mod relayer;

pub use config::Config;
pub use error::Error;
pub use reads::Quote;
pub use relayer::{Accepted, Call, Relayer, RelayerError};

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::client::evm::Rpc;

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

    pub(crate) fn rpc(&self) -> Rpc<H> {
        Rpc::new(self.http.clone(), self.config.rpc.clone())
    }
}
