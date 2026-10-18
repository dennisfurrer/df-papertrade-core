//! Papertrade SDK for Rust: perps on HyperEVM at Hyperliquid's BBO mid, gasless through a relayer.
//!
//! - [`client`]: the [`Client`], its config and reads, the relayer, the deployment and HyperEVM
//!   reads.
//! - [`markets`]: the trading state and the relayer's protocol figures.
//! - [`account`]: the live account, the session key, storage and the wallet.
//! - [`trade`]: intents, signatures, the venue's integer math and the writes.
//! - [`transfer`]: deposits from Hyperliquid.
//! - [`live`]: the account stream's event parser.
//! - [`utils`]: units, nonces and JSON readers.
//!
//! Mainnet only. Money is integers and exact decimals, never floats.
#![deny(clippy::float_arithmetic)]

pub mod account;
pub mod client;
pub mod live;
pub mod markets;
pub mod trade;
pub mod transfer;
pub mod utils;

#[cfg(test)]
pub(crate) mod test_support;

pub use client::{Client, Config, Error};
