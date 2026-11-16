//! `GET /query/builders/leaderboard` and `GET /query/builders/<address>/stats`: the builders
//! (frontends that earn a fee on the closes their users' session keys make), the registry and its
//! fee rate, and one builder's standing.

use df_hyperliquid_core::utils::{Address, Json};

use crate::utils::wire::{BadWire, address_of, array_of, bool_of, field, parse, str_of, u64_of, u128_of};

/// One builder on the leaderboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuilderRow {
    pub address: Address,
    /// As its application named it.
    pub name: Option<String>,
    pub users: u64,
    pub closes: u64,
    /// Notional closed through it, collateral units.
    pub notional: u128,
    /// Builder fees it has earned, collateral units.
    pub accrued: u128,
}

/// `GET /query/builders/leaderboard`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Builders {
    /// The BuilderRegistry, while one is live.
    pub registry: Option<Address>,
    /// The builder fee, an 18-decimal fraction.
    pub fee_rate: Option<u128>,
    /// When the figures were read, Unix ms.
    pub as_of_ms: u64,
    pub builders: Vec<BuilderRow>,
}

/// `GET /query/builders/<address>/stats`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Builder {
    pub address: Address,
    /// Approved on chain: session keys may name it.
    pub approved: bool,
    /// Earned and not claimed, collateral units.
    pub unclaimed: u128,
    pub users: u64,
    pub session_keys: u64,
    pub closes: u64,
    /// Notional closed through it, collateral units.
    pub notional: u128,
}

fn present(json: &Json, key: &str) -> Option<Json> {
    json.get(key).filter(|value| !matches!(value, Json::Null)).cloned()
}

impl Builders {
    pub fn parse(text: &str) -> Result<Builders, BadWire> {
        let json = parse(text, "builders")?;
        let registry = present(&json, "registry").map(|r| address_of(&r, "registry")).transpose()?;
        let fee_rate = present(&json, "feeRateRaw").map(|r| u128_of(&r, "feeRateRaw")).transpose()?;
        let builders = array_of(field(&json, "builders")?, "builders")?
            .iter()
            .map(|row| {
                Ok(BuilderRow {
                    address: address_of(field(row, "address")?, "address")?,
                    name: present(row, "name").map(|n| str_of(&n, "name").map(str::to_owned)).transpose()?,
                    users: u64_of(field(row, "users")?, "users")?,
                    closes: u64_of(field(row, "closes")?, "closes")?,
                    notional: u128_of(field(row, "notionalRaw")?, "notionalRaw")?,
                    accrued: u128_of(field(row, "accruedRaw")?, "accruedRaw")?,
                })
            })
            .collect::<Result<Vec<_>, BadWire>>()?;
        Ok(Builders { registry, fee_rate, as_of_ms: u64_of(field(&json, "asOfMs")?, "asOfMs")?, builders })
    }
}

impl Builder {
    pub fn parse(text: &str) -> Result<Builder, BadWire> {
        let json = parse(text, "builder")?;
        let routing = field(&json, "routing")?;
        Ok(Builder {
            address: address_of(field(&json, "address")?, "address")?,
            approved: bool_of(field(&json, "approved")?, "approved")?,
            unclaimed: u128_of(field(&json, "unclaimedRaw")?, "unclaimedRaw")?,
            users: u64_of(field(routing, "users")?, "users")?,
            session_keys: u64_of(field(routing, "sessionKeys")?, "sessionKeys")?,
            closes: u64_of(field(routing, "closes")?, "closes")?,
            notional: u128_of(field(routing, "notionalRaw")?, "notionalRaw")?,
        })
    }
}

#[cfg(test)]
#[path = "tests/builders.rs"]
mod tests;
