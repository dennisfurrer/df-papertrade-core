//! `GET /query/protocol/trades/recent`: the latest opens, closes and liquidations across the
//! venue, newest first.

use df_hyperliquid_core::utils::{Address, Dec, Json};

use crate::utils::wire::{
    BadWire, address_of, array_of, bad, bool_of, dec_of, field, hash_of, nullable, parse, str_of, u32_of, u64_of,
    u128_of,
};

/// What a recent trade was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeKind {
    Open,
    Close,
    Liquidation,
}

/// One row of `GET /query/protocol/trades/recent`. Dollar figures and prices arrive as JSON
/// floats, are read exactly as [`Dec`], and are for display only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    pub seq: u128,
    pub ts_ms: u64,
    pub wallet: Address,
    pub instrument: String,
    pub instrument_id: u32,
    pub terminal: bool,
    pub is_long: bool,
    pub leverage: u32,
    pub notional_usd: Dec,
    pub intent_id: Option<[u8; 32]>,
    pub position_id: u128,
    pub kind: TradeKind,
    pub entry: Option<Dec>,
    pub exit: Option<Dec>,
    pub pnl_usd: Option<Dec>,
}

/// Parses the recent trades, newest first.
pub fn recent_trades(text: &str) -> Result<Vec<Trade>, BadWire> {
    let json = parse(text, "trades_recent")?;
    array_of(field(&json, "events")?, "events")?.iter().map(read_trade).collect()
}

fn read_trade(row: &Json) -> Result<Trade, BadWire> {
    let kind = match str_of(field(row, "kind")?, "kind")? {
        "open" => TradeKind::Open,
        "close" => TradeKind::Close,
        "liq" | "liquidation" => TradeKind::Liquidation,
        _ => return Err(bad("kind", field(row, "kind")?)),
    };
    let is_long = match str_of(field(row, "side")?, "side")? {
        "long" => true,
        "short" => false,
        _ => return Err(bad("side", field(row, "side")?)),
    };
    Ok(Trade {
        seq: u128_of(field(row, "seq")?, "seq")?,
        ts_ms: u64_of(field(row, "ts")?, "ts")?,
        wallet: address_of(field(row, "wallet")?, "wallet")?,
        instrument: str_of(field(row, "instrument")?, "instrument")?.to_owned(),
        instrument_id: u32_of(field(row, "instrumentId")?, "instrumentId")?,
        terminal: bool_of(field(row, "instrumentTerminal")?, "instrumentTerminal")?,
        is_long,
        leverage: u32_of(field(row, "leverage")?, "leverage")?,
        notional_usd: dec_of(field(row, "notionalUsd")?, "notionalUsd")?,
        intent_id: nullable(field(row, "intentId")?, "intentId", hash_of)?,
        position_id: u128_of(field(row, "positionId")?, "positionId")?,
        kind,
        entry: nullable(field(row, "entry")?, "entry", dec_of)?,
        exit: nullable(field(row, "exit")?, "exit", dec_of)?,
        pnl_usd: nullable(field(row, "pnlUsd")?, "pnlUsd", dec_of)?,
    })
}

#[cfg(test)]
#[path = "tests/trades.rs"]
mod tests;
