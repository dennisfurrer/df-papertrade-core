//! What a new position may commit, and the checks an open must pass before it is signed:
//! pause switches, size limits, funds, queue liquidity, the account's open interest cap and the
//! side's open interest cap.

use super::mul_div;
use crate::markets::trading::{Instrument, NO_CAP, TradingState};

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
    /// The account's own open-interest cap on the market
    /// ([`Account::oi_room`](crate::account::Account::oi_room)).
    AccountOpenInterest,
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
/// capped to queue liquidity when `queue_limited` is set. `account_room` is the notional the
/// account may still open on the market ([`Account::oi_room`](crate::account::Account::oi_room)),
/// `None` for no cap or no account.
#[must_use]
pub fn open_limits(
    instrument: &Instrument,
    is_long: bool,
    leverage: u32,
    mark: Option<u128>,
    available: u128,
    queue_limited: bool,
    account_room: Option<u128>,
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
    if let Some(room) = account_room {
        tighten(room / u128::from(leverage), Limit::AccountOpenInterest, &mut limits);
    }
    let (own, cap) =
        if is_long { (instrument.long_oi, instrument.long_cap) } else { (instrument.short_oi, instrument.short_cap) };
    if let Some(mark) = mark.filter(|mark| *mark > 0)
        && cap > 0
        && cap != NO_CAP
    {
        let room = cap.saturating_sub(own);
        if let Some(headroom) = mul_div(room, mark, u128::from(instrument.price_scale)) {
            limits.headroom = Some(headroom);
            tighten(headroom / u128::from(leverage), Limit::OpenInterest, &mut limits);
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
    #[error("margin times leverage is under the venue's minimum position")]
    BelowMinimumNotional,
    #[error("the position exceeds the market limit")]
    AboveMaximumPosition,
    #[error("leverage exceeds this market's limit")]
    LeverageTooHigh,
    #[error("leverage is under the venue's minimum")]
    LeverageTooLow,
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("the margin exceeds the debt the next queue entries can fund")]
    QueueLiquidity,
    #[error("the side's open interest cap leaves no room for this position")]
    OpenInterest,
    #[error("the account's open interest cap on this market leaves no room for this position")]
    AccountOpenInterest,
}

/// Checks before signing an open: pause switches, size limits, funds, queue liquidity, the
/// account's open interest cap (`account_room`, the notional it may still open on the market, as
/// [`Account::oi_room`](crate::account::Account::oi_room) gives it; `None` for no cap or no
/// account) and the side's open interest cap at `mark`.
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
    account_room: Option<u128>,
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
    let (lowest, highest) = state.leverage_range(instrument);
    if leverage == 0 || leverage > highest {
        return Err(OpenRefusal::LeverageTooHigh);
    }
    if leverage < lowest {
        return Err(OpenRefusal::LeverageTooLow);
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
    if account_room.is_some_and(|room| notional > room) {
        return Err(OpenRefusal::AccountOpenInterest);
    }
    let limits = open_limits(instrument, is_long, leverage, mark, u128::MAX, false, None);
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

#[cfg(test)]
#[path = "tests/limits.rs"]
mod tests;
