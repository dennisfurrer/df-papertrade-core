//! Papertrade SDK for Rust: perps on HyperEVM at Hyperliquid's BBO mid, gasless through a relayer.
//!
//! - [`client`]: the relayer, the deployment, HyperEVM reads and the [`Client`].
//! - [`markets`]: the trading state, protocol summary, payout queue and recent trades.
//! - [`account`]: the live account, session keys, storage and the wallet.
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

pub use client::{Client, Config, Error};
