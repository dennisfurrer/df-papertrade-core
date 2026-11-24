//! The trading state (`GET /state/trading`): the venue's switches, minimums and leverage bounds,
//! its win fee, the staking accumulator, every [`Instrument`], the relayer's [`Intake`], the
//! account open interest cap, the Exchange implementation and the payout curve in force.

pub mod instrument;
pub mod intake;

pub use instrument::{Instrument, NO_CAP};
pub use intake::Intake;

use df_hyperliquid_core::utils::Json;

use crate::trade::math::Curve;
use crate::utils::wire::{BadWire, array_of, bad, bool_of, field, parse, str_of, u32_of, u64_of, u128_of};

/// The Exchange implementation the relayer says it settles against (`exchange`). The payout of a
/// winning close depends on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeVersion {
    /// No `exchange` field, as in the answer recorded on 2026-10-18: papertrade.xyz reads it as
    /// contract set 1 (7 to 21 Oct 2026), whose winning closes have one payout curve and no
    /// holdback band.
    Original,
    /// `"next"`: contract set 2, live since 21 Oct 2026, whose payout this crate's math follows.
    /// papertrade.xyz's config gives it the implementation
    /// `0x0a44049dbbb6b4762a6ba37aa0f7548c0b4a3723` and the alias
    /// `0x4e2adafc40c734e9a8f96458b83ad62ad2b8a964`, the one the Exchange proxy pointed at on
    /// 2026-11-24.
    Next,
    /// Any other value, as sent. Its payout is not known to this crate.
    Other(String),
}

/// `GET /state/trading`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingState {
    pub indexed_through_block: u64,
    /// Global guardian pause. No opens anywhere while set.
    pub trading_paused: bool,
    /// The least margin a position opens with, collateral units.
    pub minimum_margin: u128,
    /// The least notional (margin x leverage) a position opens with.
    pub minimum_open_notional: u128,
    /// PaperStaking's reward accumulator, scaled by 10^27.
    pub acc_reward_per_share: u128,
    pub instruments: Vec<Instrument>,
    /// `intakeControls`. With none sent, every action is accepted.
    pub intake: Option<Intake>,
    /// `minimumLeverage`, venue-wide, over each market's own. `None` when the relayer sends 0, as
    /// it does while no bound is set, or nothing.
    pub minimum_leverage: Option<u32>,
    /// `maximumLeverage`, venue-wide, over each market's own cap. `None` likewise.
    pub maximum_leverage: Option<u32>,
    /// `winFeeRateRaw`: the share of a winning close's gain the venue keeps, an 18-decimal
    /// fraction, as [`value`](crate::trade::math::value) takes it. `None` when not sent.
    pub win_fee_rate: Option<u128>,
    /// `accountOiCapRaw`: the most notional (margin x leverage, in collateral units, 18
    /// decimals) one account may hold on one market, its long and short positions and its queued
    /// opens together. An account's own `oiCaps` override it
    /// ([`Account::oi_cap`](crate::account::Account::oi_cap)). `None` when the relayer sends 0 or
    /// nothing: no cap. It is the relayer's limit: no Exchange view returns it.
    pub account_oi_cap: Option<u128>,
    /// `exchange`: the Exchange implementation the relayer settles against.
    pub exchange: ExchangeVersion,
    /// `offHours`: the off-hours keeper has switched winning closes to the off-hours payout curve
    /// (the Exchange's `offHoursPayout()` as of [`TradingState::indexed_through_block`]). The
    /// relayer sends it only while it is set, so an answer without it reads as `false`.
    pub off_hours: bool,
}

impl TradingState {
    /// Parses the answer's text.
    pub fn parse(text: &str) -> Result<TradingState, BadWire> {
        TradingState::read(&parse(text, "trading_frame")?)
    }

    /// Reads the parsed answer. Only schema version 1 is accepted, and instruments must come in
    /// rising id order with no symbol or perp index twice.
    pub fn read(json: &Json) -> Result<TradingState, BadWire> {
        let version = u64_of(field(json, "schemaVersion")?, "trading_schema")?;
        if version != 1 {
            return Err(bad("trading_schema", field(json, "schemaVersion")?));
        }
        let mut instruments = Vec::new();
        for row in array_of(field(json, "instruments")?, "trading_schema")? {
            let instrument = Instrument::read(row)?;
            let repeated = instruments.iter().any(|seen: &Instrument| {
                seen.id >= instrument.id
                    || seen.symbol == instrument.symbol
                    || seen.asset_index == instrument.asset_index
            });
            if repeated {
                return Err(bad("trading_instrument_order", row));
            }
            instruments.push(instrument);
        }
        let intake = match json.get("intakeControls") {
            None | Some(Json::Null) => None,
            Some(row) => Some(Intake::read(row)?),
        };
        let optional = |key: &str| json.get(key).filter(|value| **value != Json::Null);
        let bound = |key: &str, what: &str| -> Result<Option<u32>, BadWire> {
            Ok(optional(key).map(|value| u32_of(value, what)).transpose()?.filter(|bound| *bound > 0))
        };
        let win_fee_rate = optional("winFeeRateRaw").map(|value| u128_of(value, "trading_win_fee_rate")).transpose()?;
        let account_oi_cap = optional("accountOiCapRaw")
            .map(|value| u128_of(value, "trading_account_oi_cap"))
            .transpose()?
            .filter(|cap| *cap > 0);
        let exchange = match optional("exchange") {
            None => ExchangeVersion::Original,
            Some(value) => match str_of(value, "trading_exchange")? {
                "next" => ExchangeVersion::Next,
                other => ExchangeVersion::Other(other.to_owned()),
            },
        };
        let off_hours = optional("offHours").map(|value| bool_of(value, "trading_off_hours")).transpose()?;
        Ok(TradingState {
            indexed_through_block: u64_of(field(json, "indexedThroughBlock")?, "trading_block")?,
            trading_paused: bool_of(field(json, "tradingPaused")?, "trading_schema")?,
            minimum_margin: u128_of(field(json, "minimumTradeSizeRaw")?, "minimum_trade_size")?,
            minimum_open_notional: u128_of(field(json, "minimumOpenNotionalRaw")?, "minimum_open_notional")?,
            acc_reward_per_share: u128_of(field(json, "accRewardPerShareRaw")?, "reward_accumulator")?,
            instruments,
            intake,
            minimum_leverage: bound("minimumLeverage", "trading_minimum_leverage")?,
            maximum_leverage: bound("maximumLeverage", "trading_maximum_leverage")?,
            win_fee_rate,
            account_oi_cap,
            exchange,
            off_hours: off_hours.unwrap_or(false),
        })
    }

    /// The payout curve a winning close settles on now: the off-hours curve while
    /// [`TradingState::off_hours`] is set, else the standard one. The keeper sets one flag for the
    /// whole Exchange, so it is the same for every market. `None` unless the relayer settles
    /// against contract set 2 ([`ExchangeVersion::Next`]): the two curves are that set's.
    #[must_use]
    pub fn curve(&self) -> Option<Curve> {
        match self.exchange {
            ExchangeVersion::Next => Some(if self.off_hours { Curve::OffHours } else { Curve::Standard }),
            ExchangeVersion::Original | ExchangeVersion::Other(_) => None,
        }
    }

    /// The leverage an open on `instrument` may use, lowest and highest: the market's own cap
    /// within the venue's bounds.
    #[must_use]
    pub fn leverage_range(&self, instrument: &Instrument) -> (u32, u32) {
        let highest = self.maximum_leverage.map_or(instrument.max_leverage, |venue| venue.min(instrument.max_leverage));
        (self.minimum_leverage.unwrap_or(1).max(1), highest)
    }

    #[must_use]
    pub fn instrument(&self, id: u32) -> Option<&Instrument> {
        self.instruments.iter().find(|instrument| instrument.id == id)
    }

    /// The instrument with `symbol`, case-insensitive.
    #[must_use]
    pub fn by_symbol(&self, symbol: &str) -> Option<&Instrument> {
        self.instruments.iter().find(|instrument| instrument.symbol.eq_ignore_ascii_case(symbol))
    }

    /// Whether the relayer takes `action` now.
    #[must_use]
    pub fn accepts(&self, action: &str) -> bool {
        self.intake.as_ref().is_none_or(|intake| intake.accepts(action))
    }
}

#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
