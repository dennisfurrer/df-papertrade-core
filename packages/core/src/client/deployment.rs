//! The HyperEVM mainnet deployment (chain 999), contract set 2 (live since 2026-10-21): contract
//! addresses, listed markets, the relayer origin and the three EIP-712 domains. The exchange and staking domain separators
//! match the contracts' `DOMAIN_SEPARATOR()` as read on 2026-10-13.

use df_hyperliquid_core::utils::Address;

use df_hyperliquid_core::utils::sign::Domain;

const fn address(text: &str) -> Address {
    match Address::parse_const(text) {
        Some(address) => address,
        None => Address::ZERO,
    }
}

/// HyperEVM mainnet.
pub const CHAIN_ID: u64 = 999;

/// Holds balances and positions, settles trades and runs the payout queue. Verifying
/// contract of [`EXCHANGE_DOMAIN`] and [`CANCEL_DOMAIN`].
pub const EXCHANGE: Address = address("0x6cd5661646289fb6e65ea5c032310fded797d0a2");
/// Executes batches of signed intents, callable only by the relayer.
pub const BATCH_EXECUTOR: Address = address("0x233ff29b239abc1d7748605da9b205c3e008626e");
/// Reads Hyperliquid's BBO precompile for the execution price.
pub const PRICE_ORACLE: Address = address("0xa9b4c7b7ca5835652d81e25739c870d49f39cc2e");
/// Registers, checks and revokes session keys. Revoking is a direct wallet call paid in HYPE gas.
pub const SESSION_KEY_MANAGER: Address = address("0x8326eb8896e358717f10502ec73ce8d28e37b108");
/// The approved builders and the builder fee rate. Not a proxy. A session key registration that
/// names an address it does not approve is refused.
pub const BUILDER_REGISTRY: Address = address("0xfdc6550436dd75ce8a0b145f935b7d3a64f53248");
/// Derives each wallet's deposit address (`computeProxyAddress`) and deploys it.
pub const DEPOSIT_PROXY_FACTORY: Address = address("0x1629b7d46bef6e13ff754174fc9f32dea309b38f");
/// The PAPER token, 18 decimals. At launch it only moves into and out of staking.
pub const PAPER: Address = address("0xe40f17915daa230324030003a197cdaef2261c0e");
/// PAPER staking and its USDC rewards. Verifying contract of [`STAKING_DOMAIN`].
pub const PAPER_STAKING: Address = address("0xaad6c7b0cc3014fc80ffedae0ed7ce5967b0f016");
/// The mint curve and the stakers' share of LP excess.
pub const PAPER_TOKENOMICS: Address = address("0x75174549dbbd44497d31a47e2e2d7406408d63d9");
/// Native Circle USDC on HyperEVM, 6 decimals.
pub const USDC: Address = address("0xb88339cb7199b77e23db6e890353e22632ba630f");
/// The contract the HyperEVM deposit route moves USDC through to HyperCore. Not an ERC-20.
pub const CORE_DEPOSIT_WALLET: Address = address("0x6b9e773128f453f5c2c60935ee2de2cbc5390a24");
/// Owns the contracts. Every owner action, approving a builder too, waits 24 hours here.
pub const TIMELOCK: Address = address("0xb915fa41e824d6aa31bb833ee81220c0f2c8ea0f");
/// The 2-of-3 multisig that owns the timelock. As the BuilderRegistry's guardian it can also
/// remove a builder at once.
pub const MULTISIG: Address = address("0x696292762fd5ac3677a45edbe54eaece4194d1f6");

/// Base URL of the relayer and its indexer. A request with a foreign `Origin` header gets a
/// 400, so a browser page on another origin must go through its own server.
pub const ORIGIN: &str = "https://exchange.papertrade.xyz";

/// When the deployment started, in Unix milliseconds.
pub const GENESIS_MS: u64 = 1_791_326_520_000;

/// Domain of opens, closes, session key registrations and withdrawals. Its name is the
/// literal `String`, as the deployed contract hashes it.
pub const EXCHANGE_DOMAIN: Domain =
    Domain { name: "String", version: "1", chain_id: CHAIN_ID, verifying_contract: EXCHANGE };

/// Domain of stake, unstake and claim.
pub const STAKING_DOMAIN: Domain =
    Domain { name: "PaperStaking", version: "1", chain_id: CHAIN_ID, verifying_contract: PAPER_STAKING };

/// Domain of a cancel for a queued intent.
pub const CANCEL_DOMAIN: Domain =
    Domain { name: "PaperTrade Intent Cancel", version: "2", chain_id: CHAIN_ID, verifying_contract: EXCHANGE };

/// A market listed at launch. The live list with its parameters is in [`crate::markets::trading`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    /// The instrument id an intent names.
    pub id: u32,
    pub symbol: &'static str,
    /// Hyperliquid's perp index, the one the BBO precompile reads.
    pub asset_index: u32,
    /// A raw price is the price times this, `10^(6 - szDecimals)` as in Hyperliquid's precompiles.
    pub price_scale: u64,
}

pub const BTC: Listing = Listing { id: 0, symbol: "BTC", asset_index: 0, price_scale: 10 };
pub const ETH: Listing = Listing { id: 1, symbol: "ETH", asset_index: 1, price_scale: 100 };

/// Every market the deployment listed at launch.
pub const LISTINGS: [Listing; 2] = [BTC, ETH];

/// The listing with `symbol`, case-insensitive.
#[must_use]
pub fn listing(symbol: &str) -> Option<Listing> {
    LISTINGS.into_iter().find(|listing| listing.symbol.eq_ignore_ascii_case(symbol))
}

#[cfg(test)]
#[path = "tests/deployment.rs"]
mod tests;
