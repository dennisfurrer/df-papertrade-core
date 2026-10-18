//! A position valued at a mark: its raw PnL, what the deadband and impact curve leave of a win,
//! the win fee, whether it is liquidated and how far it has run toward bust.

use df_hyperliquid_core::client::evm::U256;

use super::mul_div;
use crate::account::state::Position;
use crate::markets::trading::Instrument;
use crate::utils::units::WAD;

/// Win fee on a winning close's gain, 18-decimal fraction (2%). Read from `winFeeRate()`
/// on 2026-10-13.
pub const WIN_FEE_RATE: u128 = 20_000_000_000_000_000;

/// The deadband is `entry / 50000`, 0.2 basis points of the entry price.
pub const DEADBAND_DIVISOR: u128 = 50_000;

/// A position valued at a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Valuation {
    /// The mark it was valued at, raw.
    pub mark: u128,
    /// Price move times notional, before impact and fees, in collateral units.
    pub raw_pnl: i128,
    /// A loss capped at the margin, or a win after the deadband and impact curve.
    pub adjusted_pnl: i128,
    /// What closing now credits beyond the margin. A win also pays the win fee, a loss
    /// equals [`Valuation::adjusted_pnl`].
    pub payout_pnl: i128,
    /// The mark is at or past the bust price, so the keeper liquidates it.
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
    win_fee_rate: u128,
) -> Result<Valuation, ValuationError> {
    if position.instrument_id != instrument.id {
        return Err(ValuationError::WrongInstrument);
    }
    let mark = if instrument.terminal { instrument.frozen_mid().ok_or(ValuationError::NoPrice)? } else { mark };
    let (raw_pnl, adjusted_pnl) = impact(position, instrument, mark)?;
    let payout_pnl = if adjusted_pnl <= 0 {
        adjusted_pnl
    } else {
        let gain = adjusted_pnl.unsigned_abs();
        let fee = mul_div(gain, win_fee_rate, WAD).ok_or(ValuationError::OutOfRange)?;
        i128::try_from(gain - fee.min(gain)).map_err(|_| ValuationError::OutOfRange)?
    };
    let liquidated = !instrument.terminal
        && if position.is_long { mark <= position.bust_price } else { mark >= position.bust_price };
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
    Ok(Valuation { mark, raw_pnl, adjusted_pnl, payout_pnl, liquidated, risk_bp })
}

/// Distance of `price` from `entry` against the position (below entry for a long).
fn signed_gap(is_long: bool, entry: u128, price: u128) -> i128 {
    let (from, to) = if is_long { (entry, price) } else { (price, entry) };
    i128::try_from(from).unwrap_or(i128::MAX) - i128::try_from(to).unwrap_or(i128::MAX)
}

/// The raw PnL and, for a win, what the impact curve leaves of it.
fn impact(position: &Position, instrument: &Instrument, mark: u128) -> Result<(i128, i128), ValuationError> {
    let entry = position.entry_price;
    let word = u128::from(u64::MAX);
    let band = entry / DEADBAND_DIVISOR;
    let past = if position.is_long { entry.checked_add(band) } else { mark.checked_add(band) };
    if entry == 0 || entry > word || mark > word || past.is_none_or(|past| past > word) {
        return Err(ValuationError::OutOfRange);
    }
    // Pull a winning exit back by the deadband. A win inside the band counts as none.
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
