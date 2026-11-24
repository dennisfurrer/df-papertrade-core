//! A position valued at a mark: its raw PnL, what the deadband, the payout curve's holdback and
//! the impact curve leave of a win, the win and builder fees, whether it is liquidated and how far
//! it has run toward bust.
//!
//! Contract set 2 (2026-10-21) holds a win on a small move back a little further, inside a band
//! of moves that depends on the payout curve in force ([`Curve`]): the off-hours keeper switches
//! the whole Exchange between the standard and the off-hours curve (`offHoursPayout()`), and the
//! relayer's trading state says which is in force
//! ([`TradingState::off_hours`](crate::markets::TradingState::off_hours)). The holdback follows the
//! docs' formula (`docs.papertrade.xyz/how/asymmetric-impact`) in 18-decimal fixed point. A close
//! at or past the bust price settles as a liquidation: the whole margin, no fees.
//!
//! [`Rates::current`] gives the rates a close signed now settles at, from the trading state and
//! the builder fee of the session key that signs it.

use df_hyperliquid_core::client::evm::U256;

use super::mul_div;
use crate::account::state::Position;
use crate::markets::trading::{ExchangeVersion, Instrument, TradingState};
use crate::utils::units::WAD;

/// Win fee on a winning close's gain, 18-decimal fraction (2%), as `winFeeRate()` and the
/// relayer's [`TradingState::win_fee_rate`](crate::markets::TradingState::win_fee_rate) both
/// read on 2026-10-18. Prefer the live figure where one is at hand.
pub const WIN_FEE_RATE: u128 = 20_000_000_000_000_000;

/// The deadband is `entry / 50000`, 0.2 basis points of the entry price.
pub const DEADBAND_DIVISOR: u128 = 50_000;

/// The builder fee on a winning close through an approved builder, 18-decimal fraction (0.5%), as
/// the BuilderRegistry's `feeRate()` read on 2026-11-16. Prefer the live figure where one is at
/// hand.
pub const BUILDER_FEE_RATE: u128 = 5_000_000_000_000_000;

/// One basis point, 18-decimal fraction.
const BP: u128 = WAD / 10_000;

/// The payout curve a winning close settles on. They differ only in the holdback band.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Curve {
    /// Holds back moves between 0.75 and 6 bps, by at most about 1.2 bps.
    #[default]
    Standard,
    /// Weekends and other quiet hours: holds back moves between 0.3 and 10 bps, by at most about
    /// 2 bps.
    OffHours,
}

impl Curve {
    /// The band's start and end, and the holdback's factor, each an 18-decimal fraction.
    const fn band(self) -> (u128, u128, u128) {
        match self {
            Curve::Standard => (75 * BP / 100, 6 * BP, 3_597_000_000_000_000_000),
            Curve::OffHours => (30 * BP / 100, 10 * BP, 6_085_000_000_000_000_000),
        }
    }

    /// How much of a move (`price_move`, an 18-decimal fraction of the entry, before the
    /// deadband) is held back: `factor x width x u^2 x (1 - u)^2` on the standard curve,
    /// `(1 - u)^3` on the off-hours one, with `u` how far through the band the move is.
    #[must_use]
    pub fn holdback(self, price_move: u128) -> u128 {
        let (start, end, factor) = self.band();
        if price_move <= start || price_move >= end {
            return 0;
        }
        let width = end - start;
        let Some(u) = mul_div(price_move - start, WAD, width) else { return 0 };
        let rest = WAD - u;
        let mut shape =
            mul_div(mul_div(u, u, WAD).unwrap_or(0), mul_div(rest, rest, WAD).unwrap_or(0), WAD).unwrap_or(0);
        if self == Curve::OffHours {
            shape = mul_div(shape, rest, WAD).unwrap_or(0);
        }
        mul_div(mul_div(factor, width, WAD).unwrap_or(0), shape, WAD).unwrap_or(0)
    }
}

/// The fees on a winning close and the curve it settles on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    /// The win fee, 18-decimal fraction.
    pub win_fee: u128,
    /// The builder fee where the position's session key names an approved builder, else zero.
    pub builder_fee: u128,
    pub curve: Curve,
}

impl Default for Rates {
    /// The win fee at contract set 2's start, no builder, the standard curve.
    fn default() -> Self {
        Rates { win_fee: WIN_FEE_RATE, builder_fee: 0, curve: Curve::Standard }
    }
}

/// Why the rates of a close cannot be told from the trading state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RatesError {
    /// The relayer settles against an Exchange whose payout this crate does not follow (only
    /// contract set 2, [`ExchangeVersion::Next`], is followed).
    #[error("the relayer settles against an Exchange this crate does not follow: {0:?}")]
    Exchange(ExchangeVersion),
}

impl Rates {
    /// The rates a winning close signed now settles at: the win fee the relayer sends
    /// ([`TradingState::win_fee_rate`], [`WIN_FEE_RATE`] when it sends none), `builder_fee` and
    /// the curve in force ([`TradingState::curve`]).
    ///
    /// `builder_fee` is the fee of the session key that signs the close, an 18-decimal fraction:
    /// the BuilderRegistry's `feeRateOf` the builder the key names, 0 when it names none or one
    /// no longer approved. [`Client::close_builder_fee`](crate::client::Client::close_builder_fee)
    /// reads it for the stored key. A close is charged by the key that signs it, not the one that
    /// opened the position, so one figure serves every position of the account.
    ///
    /// The curve is the one in force at the trading state's indexed block; the keeper may switch
    /// it before the close executes, and every close in one batch settles on one curve.
    pub fn current(state: &TradingState, builder_fee: u128) -> Result<Rates, RatesError> {
        let curve = state.curve().ok_or_else(|| RatesError::Exchange(state.exchange.clone()))?;
        Ok(Rates { win_fee: state.win_fee_rate.unwrap_or(WIN_FEE_RATE), builder_fee, curve })
    }
}

/// A position valued at a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valuation {
    /// The mark it was valued at, raw.
    pub mark: u128,
    /// Price move times notional, before impact and fees, in collateral units.
    pub raw_pnl: i128,
    /// A loss capped at the margin, or a win after the deadband and impact curve.
    pub adjusted_pnl: i128,
    /// What closing now credits beyond the margin. A win also pays the win and builder fees, a
    /// loss equals [`Valuation::adjusted_pnl`].
    pub payout_pnl: i128,
    /// The builder fee a win pays, collateral units.
    pub builder_fee: u128,
    /// The mark is at or past the bust price: the keeper liquidates it, and a close settles as a
    /// liquidation, the whole margin lost.
    pub liquidated: bool,
    /// Progress from entry toward bust in basis points (0 to 10000). `None` for a retired market.
    pub risk_bp: Option<u32>,
}

/// Why a position could not be valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ValuationError {
    #[error("the position does not belong to this instrument")]
    WrongInstrument,
    #[error("the instrument is retired and has no frozen price")]
    NoPrice,
    #[error("the price is outside what the contract takes")]
    OutOfRange,
}

/// Values a position at a raw `mark`, or at the frozen price of a retired market.
pub fn value(
    position: &Position,
    instrument: &Instrument,
    mark: u128,
    rates: Rates,
) -> Result<Valuation, ValuationError> {
    if position.instrument_id != instrument.id {
        return Err(ValuationError::WrongInstrument);
    }
    let mark = if instrument.terminal { instrument.frozen_mid().ok_or(ValuationError::NoPrice)? } else { mark };
    let liquidated = !instrument.terminal
        && if position.is_long { mark <= position.bust_price } else { mark >= position.bust_price };
    let (raw_pnl, adjusted_pnl) = impact(position, instrument, mark, rates.curve)?;
    let margin = i128::try_from(position.margin).map_err(|_| ValuationError::OutOfRange)?;
    let (adjusted_pnl, payout_pnl, builder_fee) = if liquidated {
        (-margin, -margin, 0)
    } else if adjusted_pnl <= 0 {
        (adjusted_pnl, adjusted_pnl, 0)
    } else {
        let gain = adjusted_pnl.unsigned_abs();
        let fee = mul_div(gain, rates.win_fee, WAD).ok_or(ValuationError::OutOfRange)?;
        let builder = mul_div(gain, rates.builder_fee, WAD).ok_or(ValuationError::OutOfRange)?;
        let kept = gain.saturating_sub(fee).saturating_sub(builder);
        (adjusted_pnl, i128::try_from(kept).map_err(|_| ValuationError::OutOfRange)?, builder)
    };
    let risk_bp = if instrument.terminal {
        None
    } else {
        let room = signed_gap(position.is_long, position.entry_price, position.bust_price);
        let moved = signed_gap(position.is_long, position.entry_price, mark);
        Some(if room <= 0 {
            if liquidated { 10_000 } else { 0 }
        } else if moved <= 0 {
            0
        } else if moved >= room {
            10_000
        } else {
            u32::try_from(moved * 10_000 / room).unwrap_or(10_000)
        })
    };
    Ok(Valuation { mark, raw_pnl, adjusted_pnl, payout_pnl, builder_fee, liquidated, risk_bp })
}

/// Distance of `price` from `entry` against the position (below entry for a long).
fn signed_gap(is_long: bool, entry: u128, price: u128) -> i128 {
    let (from, to) = if is_long { (entry, price) } else { (price, entry) };
    i128::try_from(from).unwrap_or(i128::MAX) - i128::try_from(to).unwrap_or(i128::MAX)
}

/// The raw PnL and, for a win, what the impact curve leaves of it.
fn impact(
    position: &Position,
    instrument: &Instrument,
    mark: u128,
    curve: Curve,
) -> Result<(i128, i128), ValuationError> {
    let entry = position.entry_price;
    let word = u128::from(u64::MAX);
    if entry == 0 || entry > word || mark > word {
        return Err(ValuationError::OutOfRange);
    }
    // the deadband, and on a win the curve's holdback, measured on the move before the deadband
    let price_move = mul_div(mark.abs_diff(entry), WAD, entry).ok_or(ValuationError::OutOfRange)?;
    let winning = if position.is_long { mark > entry } else { mark < entry };
    let held =
        if winning { mul_div(entry, curve.holdback(price_move), WAD).ok_or(ValuationError::OutOfRange)? } else { 0 };
    let band = entry / DEADBAND_DIVISOR + held;
    let past = if position.is_long { entry.checked_add(band) } else { mark.checked_add(band) };
    if past.is_none_or(|past| past > word) {
        return Err(ValuationError::OutOfRange);
    }
    // Pull a winning exit back by the deadband and holdback. A win inside them counts as none.
    let exit = if position.is_long {
        if mark > entry + band {
            mark - band
        } else if mark > entry {
            entry
        } else {
            mark
        }
    } else if mark + band < entry {
        mark + band
    } else if mark < entry {
        entry
    } else {
        mark
    };
    let gaining = if position.is_long { exit > entry } else { exit < entry };
    let moved = exit.abs_diff(entry);
    let notional = position.margin.checked_mul(u128::from(position.leverage)).ok_or(ValuationError::OutOfRange)?;
    let magnitude = mul_div(notional, moved, entry).ok_or(ValuationError::OutOfRange)?;
    let magnitude = i128::try_from(magnitude).map_err(|_| ValuationError::OutOfRange)?;
    let raw = if gaining { magnitude } else { -magnitude };
    if raw <= 0 {
        let margin = i128::try_from(position.margin).map_err(|_| ValuationError::OutOfRange)?;
        return Ok((raw, raw.max(-margin)));
    }
    let wad2 = WAD * WAD;
    let movement = mul_div(moved, WAD, entry).ok_or(ValuationError::OutOfRange)?;
    let rate_term = mul_div(movement, instrument.rate_multiplier, WAD).ok_or(ValuationError::OutOfRange)?;
    let size_term = U256::from(1_000_000u64)
        .checked_mul(U256::from(movement))
        .and_then(|product| product.checked_mul(U256::from(instrument.position_multiplier)))
        .and_then(|product| product.div_rem(U256::from(WAD)))
        .and_then(|(quotient, _)| quotient.to_u128())
        .ok_or(ValuationError::OutOfRange)?;
    if rate_term == 0 || size_term == 0 {
        return Ok((raw, 0));
    }
    let term1 = wad2 / rate_term;
    let term2 = mul_div(instrument.reference_notional, WAD, size_term).ok_or(ValuationError::OutOfRange)?;
    if term1 > wad2 || term2 > wad2 {
        return Ok((raw, 0));
    }
    let keep = WAD.checked_sub(instrument.base_rate).ok_or(ValuationError::OutOfRange)?;
    let scale = mul_div(keep, WAD, WAD + term1 + term2).ok_or(ValuationError::OutOfRange)?;
    let adjusted = mul_div(raw.unsigned_abs(), scale, WAD).ok_or(ValuationError::OutOfRange)?;
    Ok((raw, i128::try_from(adjusted).map_err(|_| ValuationError::OutOfRange)?))
}

#[cfg(test)]
#[path = "tests/valuation.rs"]
mod tests;
