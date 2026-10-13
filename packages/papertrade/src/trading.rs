//! The trading state (`GET /state/trading`): every instrument with its
//! lifecycle, impact parameters, open interest and caps; the venue-wide
//! minimums; the staking accumulator; and the relayer's intake (which actions
//! it accepts now). The app polls it every second.
//!
//! Instruments come packed as 23-field rows, read here by position as the
//! app's own decoder reads them, with the same lifecycle checks: a paused
//! market is terminal and close-only with its frozen BBO and pause time, a
//! live one has none of them.

use hl_core::Json;

use crate::wire::{BadWire, array_of, bad, bool_of, field, nullable, parse, str_of, tuple, u32_of, u64_of, u128_of};

/// A cap of `2^128 - 1` means no cap on that side.
pub const NO_CAP: u128 = u128::MAX;

/// One market, as the Exchange holds it.
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
    /// The largest notional (margin × leverage) one position may have, collateral units.
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
    /// Caps on each side's open interest in the asset; [`NO_CAP`] for none.
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
        crate::units::mid(self.frozen_bid?, self.frozen_ask?)
    }

    fn read(row: &Json) -> Result<Instrument, BadWire> {
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
            && crate::units::scale_places(instrument.price_scale).is_some();
        if !consistent {
            return Err(bad("trading_lifecycle", row));
        }
        Ok(instrument)
    }
}

/// What the relayer takes in now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intake {
    pub revision: u64,
    /// The least PAPER one stake moves, 18 decimals.
    pub paper_stake_minimum: u128,
    /// How far one side's open interest may run past the other's, collateral
    /// units; zero for no such limit.
    pub maximum_net_open_interest: u128,
    /// Actions the relayer refuses for now (`openPosition`, `registerDeposit`, …).
    pub paused: Vec<String>,
}

impl Intake {
    /// Whether the relayer takes `action` now.
    #[must_use]
    pub fn accepts(&self, action: &str) -> bool {
        !self.paused.iter().any(|paused| paused == action)
    }
}

/// `GET /state/trading`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingState {
    pub indexed_through_block: u64,
    /// The guardian's global gate: no opens anywhere while set.
    pub trading_paused: bool,
    /// The least margin a position opens with, collateral units.
    pub minimum_margin: u128,
    /// The least notional (margin × leverage) a position opens with.
    pub minimum_open_notional: u128,
    /// PaperStaking's reward accumulator, scaled by 10^27.
    pub acc_reward_per_share: u128,
    pub instruments: Vec<Instrument>,
    /// Absent from an older relayer; then everything is taken.
    pub intake: Option<Intake>,
}

impl TradingState {
    /// Reads the answer's text.
    pub fn parse(text: &str) -> Result<TradingState, BadWire> {
        TradingState::read(&parse(text, "trading_frame")?)
    }

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
        let intake = match json.get("intake") {
            None | Some(Json::Null) => None,
            Some(row) => {
                let f = tuple(row, 4, "trading_intake")?;
                let paused = array_of(&f[3], "trading_intake")?
                    .iter()
                    .map(|action| str_of(action, "trading_intake_action").map(str::to_owned))
                    .collect::<Result<_, _>>()?;
                Some(Intake {
                    revision: u64_of(&f[0], "trading_intake_revision")?,
                    paper_stake_minimum: u128_of(&f[1], "trading_paper_stake_minimum")?,
                    maximum_net_open_interest: u128_of(&f[2], "trading_net_open_interest")?,
                    paused,
                })
            }
        };
        Ok(TradingState {
            indexed_through_block: u64_of(field(json, "indexedThroughBlock")?, "trading_block")?,
            trading_paused: bool_of(field(json, "tradingPaused")?, "trading_schema")?,
            minimum_margin: u128_of(field(json, "minimumTradeSizeRaw")?, "minimum_trade_size")?,
            minimum_open_notional: u128_of(field(json, "minimumOpenNotionalRaw")?, "minimum_open_notional")?,
            acc_reward_per_share: u128_of(field(json, "accRewardPerShareRaw")?, "reward_accumulator")?,
            instruments,
            intake,
        })
    }

    #[must_use]
    pub fn instrument(&self, id: u32) -> Option<&Instrument> {
        self.instruments.iter().find(|instrument| instrument.id == id)
    }

    /// By symbol, in any case.
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
#[path = "tests/trading.rs"]
mod tests;
