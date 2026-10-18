//! What the mocked and live suites share: the recordings on disk, a pinned clock and randomness,
//! the vectors' keys, and [`venue::Venue`], a fake papertrade and HyperEVM that answer from
//! recordings.

// Each suite uses its own part of this.
#![allow(dead_code)]

pub mod venue;

use std::path::PathBuf;

use df_hyperliquid_core::account::LocalKey;
use df_hyperliquid_core::utils::Address;
use df_papertrade_core::account::LocalWallet;
use df_papertrade_core::{Client, Config};

use venue::Venue;

/// The clock the recordings were read at, in ms since the epoch.
pub const NOW: u64 = 1_791_860_400_000;

pub const fn clock() -> u64 {
    NOW
}

/// Fills with sevens: a valid session secret and a known first nonce.
pub fn sevens(bytes: &mut [u8]) -> Result<(), String> {
    bytes.fill(7);
    Ok(())
}

/// The viem vectors' wallet key: the account's owner. A published test key.
pub const WALLET_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";

pub fn wallet() -> LocalWallet {
    LocalWallet(LocalKey::from_hex(WALLET_KEY).unwrap())
}

pub fn owner() -> Address {
    wallet().0.address()
}

/// Mainnet's endpoints at the pinned clock and randomness.
pub fn config() -> Config {
    Config::mainnet(clock, sevens)
}

/// A client on `venue`.
pub fn client(venue: &Venue) -> Client<Venue> {
    Client::new(venue.clone(), config())
}

/// A file under `fixtures/`: what the venue answered, recorded.
pub fn fixture(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
