//! The deployment on HyperEVM mainnet (chain 999): every contract, the
//! instruments it lists, the relayer's origin, and the three EIP-712 domains
//! its intents are signed under. Mainnet only: Papertrade names no testnet.
//!
//! Read from exchange.papertrade.xyz's deployment config and the docs' contract
//! page, and checked on chain (2026-10-13): the session key manager's and
//! PaperStaking's `DOMAIN_SEPARATOR()` answer exactly [`EXCHANGE_DOMAIN`]'s and
//! [`STAKING_DOMAIN`]'s separators. The contracts are proxies whose sources are
//! not published; the addresses here are the ones to call.

use hl_core::{Address, Domain};

const fn address(text: &str) -> Address {
    match Address::parse_const(text) {
        Some(address) => address,
        None => Address::ZERO,
    }
}

/// HyperEVM mainnet.
pub const CHAIN_ID: u64 = 999;

/// Balances, positions, settlement and the payout queue. Every trading intent
/// names it as its verifying contract.
pub const EXCHANGE: Address = address("0x6cd5661646289fb6e65ea5c032310fded797d0a2");
/// Takes batches of signed intents from the relayer's keys only.
pub const BATCH_EXECUTOR: Address = address("0x233ff29b239abc1d7748605da9b205c3e008626e");
/// Reads Hyperliquid's BBO precompile for the execution price.
pub const PRICE_ORACLE: Address = address("0xa9b4c7b7ca5835652d81e25739c870d49f39cc2e");
/// Registers, checks and revokes session keys. Revocation is the one call a
/// wallet makes itself, and it costs HYPE for gas.
pub const SESSION_KEY_MANAGER: Address = address("0x8326eb8896e358717f10502ec73ce8d28e37b108");
/// Derives each wallet's deposit address (`computeProxyAddress`) and deploys it.
pub const DEPOSIT_PROXY_FACTORY: Address = address("0x1629b7d46bef6e13ff754174fc9f32dea309b38f");
/// The PAPER token, 18 decimals; at launch it moves only into and out of staking.
pub const PAPER: Address = address("0xe40f17915daa230324030003a197cdaef2261c0e");
/// PAPER staking and its USDC rewards; signs under [`STAKING_DOMAIN`].
pub const PAPER_STAKING: Address = address("0xaad6c7b0cc3014fc80ffedae0ed7ce5967b0f016");
/// The mint curve and the stakers' share of LP excess.
pub const PAPER_TOKENOMICS: Address = address("0x75174549dbbd44497d31a47e2e2d7406408d63d9");
/// Native Circle USDC on HyperEVM, 6 decimals.
pub const USDC: Address = address("0xb88339cb7199b77e23db6e890353e22632ba630f");
/// What the app's config calls its collateral: the contract the HyperEVM
/// deposit route moves USDC through to HyperCore. Not an ERC-20 (it answers no
/// `symbol` or `decimals`).
pub const CORE_DEPOSIT_WALLET: Address = address("0x6b9e773128f453f5c2c60935ee2de2cbc5390a24");
/// Owns the contracts; every owner action waits seven days here.
pub const TIMELOCK: Address = address("0xb915fa41e824d6aa31bb833ee81220c0f2c8ea0f");
/// The 2-of-3 multisig that owns the timelock.
pub const MULTISIG: Address = address("0x696292762fd5ac3677a45edbe54eaece4194d1f6");

/// The relayer and the indexer behind it. It answers same-origin requests
/// only: a browser on another origin is refused (a foreign `Origin` is a 400),
/// so a page elsewhere reaches it through its own server.
pub const ORIGIN: &str = "https://exchange.papertrade.xyz";

/// When the deployment started, in Unix milliseconds.
pub const GENESIS_MS: u64 = 1_791_326_520_000;

/// Trading intents: open, close, session keys and withdrawals. The name is the
/// literal `String`, as the deployed contract hashes it.
pub const EXCHANGE_DOMAIN: Domain =
    Domain { name: "String", version: "1", chain_id: CHAIN_ID, verifying_contract: EXCHANGE };

/// Stake, unstake and claim.
pub const STAKING_DOMAIN: Domain =
    Domain { name: "PaperStaking", version: "1", chain_id: CHAIN_ID, verifying_contract: PAPER_STAKING };

/// Cancelling a queued intent before the relayer sends it.
pub const CANCEL_DOMAIN: Domain =
    Domain { name: "PaperTrade Intent Cancel", version: "2", chain_id: CHAIN_ID, verifying_contract: EXCHANGE };

/// A market as the deployment lists it. The live list, with its parameters and
/// open interest, is the trading state's ([`crate::trading`]); these are the
/// ones the app checks that list against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    /// The instrument id an intent names.
    pub id: u32,
    pub symbol: &'static str,
    /// Hyperliquid's perp index, the one the BBO precompile reads.
    pub asset_index: u32,
    /// A price's raw integer is the price times this: `10^(6 - szDecimals)`,
    /// the units of Hyperliquid's precompiles.
    pub price_scale: u64,
}

pub const BTC: Listing = Listing { id: 0, symbol: "BTC", asset_index: 0, price_scale: 10 };
pub const ETH: Listing = Listing { id: 1, symbol: "ETH", asset_index: 1, price_scale: 100 };

/// Every market the deployment listed at launch.
pub const LISTINGS: [Listing; 2] = [BTC, ETH];

/// The listing by its symbol, in any case.
#[must_use]
pub fn listing(symbol: &str) -> Option<Listing> {
    LISTINGS.into_iter().find(|listing| listing.symbol.eq_ignore_ascii_case(symbol))
}

#[cfg(test)]
#[path = "tests/deployment.rs"]
mod tests;
