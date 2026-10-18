//! Recorded answers and keys. The account streams are captures of a busy wallet over five
//! minutes and of a wallet with a queued close. The keys are the viem vectors' wallet and
//! session key.

use crate::markets::trading::{Instrument, TradingState};

/// `GET /state/trading` with BTC and ETH listed.
pub(crate) const TRADING: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/trading.json"));
/// A busy wallet's account stream over five minutes.
pub(crate) const STREAM: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/live_stream.txt"));
/// A snapshot with open positions, then a close queued behind 36 other intents.
pub(crate) const QUEUED: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/live_queued_close.txt"));

/// The clock the tests run at, in ms since the epoch.
pub(crate) const NOW_MS: u64 = 1_791_860_400_000;
/// The vectors' wallet key, the account's owner.
pub(crate) const WALLET_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
/// The vectors' session key secret.
pub(crate) const SESSION_SECRET: &str = "0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
/// The session key's address, as the vectors write it.
pub(crate) const SESSION_ADDRESS: &str = "0x2c7536e3605d9c16a7a3d7b1898e529396a65c23";

/// BTC from the recorded trading state.
pub(crate) fn btc() -> Instrument {
    TradingState::parse(TRADING).unwrap().instruments.remove(0)
}
