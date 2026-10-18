//! The trading state (`GET /state/trading`): the venue's switches, minimums and leverage bounds,
//! its win fee, the staking accumulator, every [`Instrument`] and the relayer's [`Intake`].

pub mod instrument;
pub mod intake;

pub use instrument::{Instrument, NO_CAP};
pub use intake::Intake;

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, array_of, bad, bool_of, field, parse, u32_of, u64_of, u128_of};

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
        })
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
