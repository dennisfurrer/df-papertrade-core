//! Papertrade's integer math, following the Exchange's `QueueLib`: bust prices, PnL after
//! the impact curve and win fee, open limits and the PAPER mint rate. Every division rounds
//! toward zero, and wide products go through [`df_hyperliquid_core::client::evm::U256`].

use df_hyperliquid_core::client::evm::U256;

use crate::account::state::Position;

use crate::markets::trading::{Instrument, NO_CAP, TradingState};

use crate::utils::units::WAD;

/// Win fee on a winning close's gain, 18-decimal fraction (2%). Read from `winFeeRate()`
/// on 2026-10-13.
pub const WIN_FEE_RATE: u128 = 20_000_000_000_000_000;

/// The deadband is `entry / 50000`, 0.2 basis points of the entry price.
pub const DEADBAND_DIVISOR: u128 = 50_000;

/// `a * b / c` with a 256-bit intermediate, rounded toward zero. `None` for a zero divisor
/// or a result past 128 bits.
#[must_use]
pub fn mul_div(a: u128, b: u128, c: u128) -> Option<u128> {
    let (quotient, _) = U256::from(a).checked_mul(U256::from(b))?.div_rem(U256::from(c))?;
    quotient.to_u128()
}

/// Bust price for a raw `entry` and `leverage`: the entry moved `1 / leverage` against the
/// position, then `bust_buffer` back toward it.
#[must_use]
pub fn bust_price(entry: u128, leverage: u32, is_long: bool, bust_buffer: u128) -> Option<u128> {
    if leverage == 0 {
        return None;
    }
    let step = entry / u128::from(leverage);
    if is_long {
        let zero = entry.checked_sub(step)?;
        zero.checked_add(mul_div(zero, bust_buffer, WAD)?)
    } else {
        let zero = entry.checked_add(step)?;
        zero.checked_sub(mul_div(zero, bust_buffer, WAD)?)
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

/// What limits a new position's margin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The available (or queued) balance.
    Collateral,
    /// What the next queue entries can fund, for an open from the queued balance.
    QueueLiquidity,
    /// More leverage than the market allows.
    Leverage,
    /// The market's largest position.
    MarketMaxSize,
    /// The side's open-interest cap.
    OpenInterest,
}

/// The most margin a new position may commit, and what limits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenLimits {
    pub maximum: u128,
    pub limit: Limit,
    /// Notional room left under the side's cap, in collateral units. `None` with no cap or no mark.
    pub headroom: Option<u128>,
}

/// Open limits for `leverage` at a raw `mark`. `available` is the usable balance, already
/// capped to queue liquidity when `queue_limited` is set.
#[must_use]
pub fn open_limits(
    state: &TradingState,
    instrument: &Instrument,
    is_long: bool,
    leverage: u32,
    mark: Option<u128>,
    available: u128,
    queue_limited: bool,
) -> OpenLimits {
    let mut limits = OpenLimits {
        maximum: available,
        limit: if queue_limited { Limit::QueueLiquidity } else { Limit::Collateral },
        headroom: None,
    };
    let tighten = |maximum: u128, limit: Limit, limits: &mut OpenLimits| {
        if maximum < limits.maximum {
            limits.maximum = maximum;
            limits.limit = limit;
        }
    };
    if leverage == 0 || leverage > instrument.max_leverage {
        tighten(0, Limit::Leverage, &mut limits);
        return limits;
    }
    if instrument.max_position_notional > 0 {
        tighten(instrument.max_position_notional / u128::from(leverage), Limit::MarketMaxSize, &mut limits);
    }
    let (own, other, cap) = if is_long {
        (instrument.long_oi, instrument.short_oi, instrument.long_cap)
    } else {
        (instrument.short_oi, instrument.long_oi, instrument.short_cap)
    };
    if let Some(mark) = mark.filter(|mark| *mark > 0) {
        let net = state
            .intake
            .as_ref()
            .map(|intake| intake.maximum_net_open_interest)
            .filter(|net| *net > 0)
            .and_then(|net| mul_div(net, u128::from(instrument.price_scale), mark))
            .map(|net| other.saturating_add(net));
        let bound = match net {
            Some(net) if cap > 0 && cap != NO_CAP => Some(cap.min(net)),
            Some(net) => Some(net),
            None if cap > 0 && cap != NO_CAP => Some(cap),
            None => None,
        };
        if let Some(bound) = bound {
            let room = bound.saturating_sub(own);
            if let Some(headroom) = mul_div(room, mark, u128::from(instrument.price_scale)) {
                limits.headroom = Some(headroom);
                tighten(headroom / u128::from(leverage), Limit::OpenInterest, &mut limits);
            }
        }
    }
    limits
}

/// Why an open would be refused before signing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenRefusal {
    #[error("trading is paused")]
    TradingPaused,
    #[error("this market is not open for new positions")]
    MarketClosed,
    #[error("the relayer is not taking opens right now")]
    IntakePaused,
    #[error("the margin is under the venue's minimum")]
    BelowMinimumMargin,
    #[error("margin × leverage is under the venue's minimum position")]
    BelowMinimumNotional,
    #[error("the position exceeds the market limit")]
    AboveMaximumPosition,
    #[error("leverage exceeds this market's limit")]
    LeverageTooHigh,
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("the margin exceeds the debt the next queue entries can fund")]
    QueueLiquidity,
    #[error("the side's open interest cap leaves no room for this position")]
    OpenInterest,
}

/// Checks before signing an open: pause switches, size limits, funds, queue liquidity and
/// the side's open interest cap at `mark`.
#[allow(clippy::too_many_arguments)]
pub fn check_open(
    state: &TradingState,
    instrument: &Instrument,
    is_long: bool,
    margin: u128,
    leverage: u32,
    mark: Option<u128>,
    available: u128,
    debt_room: Option<u128>,
) -> Result<(), OpenRefusal> {
    if state.trading_paused {
        return Err(OpenRefusal::TradingPaused);
    }
    if !instrument.openable() {
        return Err(OpenRefusal::MarketClosed);
    }
    if !state.accepts("openPosition") {
        return Err(OpenRefusal::IntakePaused);
    }
    if leverage == 0 || leverage > instrument.max_leverage {
        return Err(OpenRefusal::LeverageTooHigh);
    }
    if margin < state.minimum_margin {
        return Err(OpenRefusal::BelowMinimumMargin);
    }
    let notional = margin.saturating_mul(u128::from(leverage));
    if instrument.max_position_notional > 0 && notional > instrument.max_position_notional {
        return Err(OpenRefusal::AboveMaximumPosition);
    }
    if notional < state.minimum_open_notional {
        return Err(OpenRefusal::BelowMinimumNotional);
    }
    if margin > available {
        return Err(OpenRefusal::InsufficientFunds);
    }
    if debt_room.is_some_and(|room| margin > room) {
        return Err(OpenRefusal::QueueLiquidity);
    }
    let limits = open_limits(state, instrument, is_long, leverage, mark, u128::MAX, false);
    if limits.limit == Limit::OpenInterest && margin > limits.maximum {
        return Err(OpenRefusal::OpenInterest);
    }
    Ok(())
}

/// How much of an open from the queued balance the queue can fund: the consumable debt
/// minus what other intents already reserve.
#[must_use]
pub const fn debt_room(consumable_debt: u128, queued: u128, available_queued: u128) -> u128 {
    consumable_debt.saturating_sub(queued.saturating_sub(available_queued))
}

/// PAPER minted per dollar of loss, 18-decimal fixed point. Flat below the cliff, then
/// `rate * (S / (S + H))^2` with `S` the decay scale and `H` the tail progress.
#[must_use]
pub fn mint_rate(tracked_lp: i128, cliff: u128, rate: u128, decay: u128, tail_progress: u128) -> u128 {
    let flat = tracked_lp < 0 || tracked_lp.unsigned_abs() < cliff;
    if flat {
        return rate;
    }
    let total = decay.saturating_add(tail_progress);
    let Some(once) = mul_div(rate, decay, total) else {
        return 0;
    };
    mul_div(once, decay, total).unwrap_or(0)
}

/// PAPER (18 decimals) a loss of `loss` collateral units mints at `rate`.
#[must_use]
pub fn paper_for_loss(loss: u128, rate: u128) -> u128 {
    mul_div(loss, rate, WAD).unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/math.rs"]
mod tests;
