//! One market of the trading state: its listing, lifecycle, impact curve, limits and open
//! interest, read from a 23-field positional row and refused if its lifecycle does not add up.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, bad, bool_of, nullable, str_of, tuple, u32_of, u64_of, u128_of};

/// A cap of `2^128 - 1` means no cap on that side.
pub const NO_CAP: u128 = u128::MAX;

/// One market. A paused market is terminal and close-only with a frozen BBO and pause time.
/// A live one has none of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    pub id: u32,
    pub symbol: String,
    /// Hyperliquid's perp index, the one the BBO precompile reads.
    pub asset_index: u32,
    /// A raw price is the price times this.
    pub price_scale: u64,
    pub active: bool,
    pub paused: bool,
    /// Retired for good: closes and liquidations settle at the frozen BBO.
    pub terminal: bool,
    pub retired: bool,
    pub close_only: bool,
    /// The BBO captured when the market was retired.
    pub frozen_bid: Option<u128>,
    pub frozen_ask: Option<u128>,
    /// When it was retired, Unix milliseconds.
    pub paused_at: Option<u64>,
    /// The impact curve's `rateMultiplier`, 18-decimal fixed point.
    pub rate_multiplier: u128,
    /// The impact curve's `positionMultiplier`, 18-decimal fixed point.
    pub position_multiplier: u128,
    pub max_leverage: u32,
    /// The largest notional (margin x leverage) one position may have, collateral units.
    pub max_position_notional: u128,
    /// How far inside the zero-equity price the bust price sits, 18-decimal fraction.
    pub bust_buffer: u128,
    /// The share of a win the impact curve never pays, 18-decimal fraction.
    pub base_rate: u128,
    /// The impact curve's `referenceNotional`, collateral units.
    pub reference_notional: u128,
    /// Open interest in the asset, 18 decimals.
    pub long_oi: u128,
    pub short_oi: u128,
    /// Caps on each side's open interest in the asset, [`NO_CAP`] for none.
    pub long_cap: u128,
    pub short_cap: u128,
}

impl Instrument {
    /// Whether a new position may open here.
    #[must_use]
    pub const fn openable(&self) -> bool {
        self.active && !self.paused
    }

    /// The frozen mid a retired market settles at.
    #[must_use]
    pub fn frozen_mid(&self) -> Option<u128> {
        crate::utils::units::mid(self.frozen_bid?, self.frozen_ask?)
    }

    pub(super) fn read(row: &Json) -> Result<Instrument, BadWire> {
        let f = tuple(row, 23, "trading_instrument_tuple")?;
        let instrument = Instrument {
            id: u32_of(&f[0], "trading_instrument_id")?,
            symbol: str_of(&f[1], "trading_symbol")?.to_owned(),
            asset_index: u32_of(&f[2], "trading_asset_index")?,
            price_scale: u64_of(&f[3], "trading_price_scale")?,
            active: bool_of(&f[4], "trading_lifecycle")?,
            paused: bool_of(&f[5], "trading_lifecycle")?,
            terminal: bool_of(&f[6], "trading_lifecycle")?,
            retired: bool_of(&f[7], "trading_lifecycle")?,
            close_only: bool_of(&f[8], "trading_lifecycle")?,
            frozen_bid: nullable(&f[9], "trading_frozen_bid", u128_of)?,
            frozen_ask: nullable(&f[10], "trading_frozen_ask", u128_of)?,
            paused_at: nullable(&f[11], "trading_paused_at", u64_of)?,
            rate_multiplier: u128_of(&f[12], "trading_rate_multiplier")?,
            position_multiplier: u128_of(&f[13], "trading_position_multiplier")?,
            max_leverage: u32_of(&f[14], "trading_max_leverage")?,
            max_position_notional: u128_of(&f[15], "trading_max_position")?,
            bust_buffer: u128_of(&f[16], "trading_bust_buffer")?,
            base_rate: u128_of(&f[17], "trading_base_rate")?,
            reference_notional: u128_of(&f[18], "trading_reference_notional")?,
            long_oi: u128_of(&f[19], "trading_long_oi")?,
            short_oi: u128_of(&f[20], "trading_short_oi")?,
            long_cap: u128_of(&f[21], "trading_long_cap")?,
            short_cap: u128_of(&f[22], "trading_short_cap")?,
        };
        let frozen = [instrument.frozen_bid.is_some(), instrument.frozen_ask.is_some(), instrument.paused_at.is_some()];
        let consistent = frozen.iter().all(|set| *set == instrument.terminal)
            && instrument.terminal == instrument.paused
            && instrument.close_only == instrument.paused
            && instrument.retired == (instrument.paused || !instrument.active)
            && instrument.price_scale > 0
            && crate::utils::units::scale_places(instrument.price_scale).is_some();
        if !consistent {
            return Err(bad("trading_lifecycle", row));
        }
        Ok(instrument)
    }
}

#[cfg(test)]
#[path = "tests/instrument.rs"]
mod tests;
