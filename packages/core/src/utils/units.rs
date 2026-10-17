//! Papertrade's integer units and conversions. Collateral is 18 decimals, USDC 6, PAPER 18,
//! and rates are 18-decimal fixed point. Prices are raw at each market's `priceScale` (10 for
//! BTC, 100 for ETH), as in Hyperliquid's precompiles.

use df_hyperliquid_core::utils::{Dec, Round};

/// Decimals of collateral amounts inside the Exchange.
pub const COLLATERAL_DECIMALS: u32 = 18;
/// Decimals of USDC on HyperEVM and HyperCore.
pub const USDC_DECIMALS: u32 = 6;
/// Decimals of PAPER.
pub const PAPER_DECIMALS: u32 = 18;
/// Collateral units in one USDC unit: `10^(18 - 6)`.
pub const COLLATERAL_PER_USDC: u128 = 1_000_000_000_000;
/// One, in 18-decimal fixed point.
pub const WAD: u128 = 1_000_000_000_000_000_000;

/// `raw / 10^decimals` exactly, or `None` past what a [`Dec`] holds.
#[must_use]
pub fn to_dec(raw: u128, decimals: u32) -> Option<Dec> {
    Dec::new(i128::try_from(raw).ok()?, decimals)
}

/// A signed amount (a PnL), `raw / 10^decimals`.
#[must_use]
pub fn signed_to_dec(raw: i128, decimals: u32) -> Option<Dec> {
    Dec::new(raw, decimals)
}

/// The raw integer of a non-negative amount at `decimals`, rounded toward zero so it never
/// exceeds the input.
#[must_use]
pub fn from_dec(value: Dec, decimals: u32) -> Option<u128> {
    if value.is_negative() {
        return None;
    }
    u128::try_from(value.units_at(decimals, Round::Down)?).ok()
}

/// Collateral units as dollars.
#[must_use]
pub fn usd(raw: u128) -> Option<Dec> {
    to_dec(raw, COLLATERAL_DECIMALS)
}

/// Dollars as collateral units, toward zero.
#[must_use]
pub fn collateral(dollars: Dec) -> Option<u128> {
    from_dec(dollars, COLLATERAL_DECIMALS)
}

/// A USDC amount (6 decimals) as collateral units.
#[must_use]
pub const fn usdc_to_collateral(usdc: u64) -> u128 {
    usdc as u128 * COLLATERAL_PER_USDC
}

/// Collateral units as USDC units (6 decimals), rounded toward zero.
#[must_use]
pub fn collateral_to_usdc(raw: u128) -> Option<u64> {
    u64::try_from(raw / COLLATERAL_PER_USDC).ok()
}

/// Decimal places of a price scale (10 is one, 100 is two). `None` unless it is a power of ten.
#[must_use]
pub fn scale_places(scale: u64) -> Option<u32> {
    let mut places = 0;
    let mut rest = scale;
    while rest > 1 {
        if !rest.is_multiple_of(10) {
            return None;
        }
        rest /= 10;
        places += 1;
    }
    (rest == 1).then_some(places)
}

/// A raw price as its decimal: `raw / scale`.
#[must_use]
pub fn price(raw: u128, scale: u64) -> Option<Dec> {
    to_dec(raw, scale_places(scale)?)
}

/// A price's raw integer at `scale`. `None` if it has more decimal places than the scale allows.
#[must_use]
pub fn price_raw(price: Dec, scale: u64) -> Option<u128> {
    let places = scale_places(scale)?;
    if price.places() > places {
        return None;
    }
    from_dec(price, places)
}

/// The oracle's execution price, `(bid + ask) / 2` in raw units rounded toward zero. `None`
/// for an empty or crossed book, which the oracle refuses.
#[must_use]
pub fn mid(bid: u128, ask: u128) -> Option<u128> {
    if bid == 0 || ask == 0 || ask < bid {
        return None;
    }
    Some(bid / 2 + ask / 2 + (bid % 2 + ask % 2) / 2)
}

#[cfg(test)]
#[path = "tests/units.rs"]
mod tests;
