//! The protocol's public figures from the relayer's query service: the
//! summary (balances, activity, fees, PAPER), each market's day volume, the
//! payout queue, recent trades and an account's place in the queue.
//!
//! Amounts are collateral units (18 decimals) unless named otherwise. The
//! recent-trades feed writes its dollar figures and prices as JSON floats;
//! they are read here exactly as written ([`hl_core::Dec`]), never as floats,
//! and are for display: the account stream's integers are the ones to count.

use hl_core::{Address, Dec, Json};

use crate::wire::{
    BadWire, address_of, array_of, bad, bool_of, dec_of, field, hash_of, i128_of, nullable, parse, str_of, u32_of,
    u64_of, u128_of,
};

/// `GET /query/protocol/summary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub block: u64,
    /// When the figures were read, Unix milliseconds.
    pub at_ms: u64,
    pub genesis_ms: u64,
    /// Everything the Exchange holds.
    pub tvl: u128,
    /// Margin in open positions.
    pub margin: u128,
    /// The LP's cash.
    pub lp: u128,
    pub reserve: u128,
    /// Owed to queued winners.
    pub queue: u128,
    /// Notional traded since genesis.
    pub volume: u128,
    pub open_positions: u64,
    pub fees_pending: u128,
    pub fees_lifetime: u128,
    pub paper: PaperFigures,
}

/// PAPER's supply, staking and mint curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperFigures {
    pub supply: u128,
    pub staked: u128,
    /// The running tally of LP profit and loss the curve reads; negative while
    /// the LP is under water.
    pub tracked_lp: i128,
    /// PaperStaking's accumulator, scaled by 10^27.
    pub accumulator: u128,
    /// The curve's high-water mark past the cliff.
    pub tail_progress: u128,
    pub reserve: u128,
    pub unallocated: u128,
    pub pending_rewards: u128,
    /// Tracked LP under which the rate is flat.
    pub cliff: u128,
    /// Tracked LP past which LP gains go to stakers.
    pub cap: u128,
    pub excess: u128,
    /// The flat rate, PAPER per dollar, 18 decimals.
    pub rate: u128,
    /// The tail's decay scale.
    pub decay: u128,
}

impl PaperFigures {
    /// PAPER per dollar of loss now, 18 decimals.
    #[must_use]
    pub fn mint_rate(&self) -> u128 {
        crate::math::mint_rate(self.tracked_lp, self.cliff, self.rate, self.decay, self.tail_progress)
    }
}

impl Summary {
    pub fn parse(text: &str) -> Result<Summary, BadWire> {
        let json = parse(text, "summary")?;
        let source = field(&json, "source")?;
        let balances = field(&json, "balances")?;
        let activity = field(&json, "activity")?;
        let fees = field(&json, "fees")?;
        let paper = field(&json, "paper")?;
        let amount = |json: &Json, key: &str| u128_of(field(json, key)?, key);
        Ok(Summary {
            block: u64_of(field(source, "block")?, "block")?,
            at_ms: u64_of(field(source, "at")?, "at")?,
            genesis_ms: u64_of(field(source, "genesis")?, "genesis")?,
            tvl: amount(balances, "tvl")?,
            margin: amount(balances, "margin")?,
            lp: amount(balances, "lp")?,
            reserve: amount(balances, "reserve")?,
            queue: amount(balances, "queue")?,
            volume: amount(activity, "volume")?,
            open_positions: u64_of(field(activity, "open")?, "open")?,
            fees_pending: amount(fees, "pending")?,
            fees_lifetime: amount(fees, "lifetime")?,
            paper: PaperFigures {
                supply: amount(paper, "supply")?,
                staked: amount(paper, "staked")?,
                tracked_lp: i128_of(field(paper, "trackedLp")?, "trackedLp")?,
                accumulator: amount(paper, "accumulator")?,
                tail_progress: amount(paper, "tailProgress")?,
                reserve: amount(paper, "reserve")?,
                unallocated: amount(paper, "unallocated")?,
                pending_rewards: amount(paper, "pendingRewards")?,
                cliff: amount(paper, "cliff")?,
                cap: amount(paper, "cap")?,
                excess: amount(paper, "excess")?,
                rate: amount(paper, "rate")?,
                decay: amount(paper, "decay")?,
            },
        })
    }
}

/// `GET /query/protocol/market-stats`: each instrument's notional over 24 hours.
pub fn market_volumes(text: &str) -> Result<Vec<(u32, u128)>, BadWire> {
    let json = parse(text, "market_stats")?;
    let markets = field(&json, "markets")?;
    let Some(rows) = markets.as_object() else {
        return Err(bad("markets", markets));
    };
    let mut volumes = rows
        .iter()
        .map(|(id, row)| {
            let id = id.parse().map_err(|_| bad("market_id", &Json::String(id.clone())))?;
            Ok((id, u128_of(field(row, "volume_24h")?, "volume_24h")?))
        })
        .collect::<Result<Vec<_>, BadWire>>()?;
    volumes.sort_by_key(|(id, _)| *id);
    Ok(volumes)
}

/// `GET /query/protocol/queue-summary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSummary {
    pub block: u64,
    /// Owed to every queued winner.
    pub total: u128,
    pub count: u64,
    /// The largest and the front entries, as the service writes them.
    pub largest: Option<Json>,
    pub front: Option<Json>,
}

impl QueueSummary {
    pub fn parse(text: &str) -> Result<QueueSummary, BadWire> {
        let json = parse(text, "queue_summary")?;
        let present = |key: &str| json.get(key).filter(|value| !matches!(value, Json::Null)).cloned();
        Ok(QueueSummary {
            block: u64_of(field(field(&json, "source")?, "block")?, "block")?,
            total: u128_of(field(&json, "total")?, "total")?,
            count: u64_of(field(&json, "count")?, "count")?,
            largest: present("largest"),
            front: present("front"),
        })
    }
}

/// What a recent trade was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeKind {
    Open,
    Close,
    Liquidation,
}

/// One row of `GET /query/protocol/trades/recent`, newest first.
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

/// The recent trades.
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

/// `GET /queue-rank/accounts/<wallet>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRank {
    pub revision: Option<u128>,
    /// Each queued intent of the wallet's and its place in the relayer's queue.
    pub entries: Vec<([u8; 32], u64)>,
}

impl QueueRank {
    pub fn parse(text: &str) -> Result<QueueRank, BadWire> {
        let json = parse(text, "queue_rank")?;
        let revision = match json.get("queueRevision") {
            Some(value) => Some(u128_of(value, "queueRevision")?),
            None => None,
        };
        let entries = array_of(field(&json, "entries")?, "entries")?
            .iter()
            .map(|entry| {
                Ok((hash_of(field(entry, "intentId")?, "intentId")?, u64_of(field(entry, "position")?, "position")?))
            })
            .collect::<Result<_, BadWire>>()?;
        Ok(QueueRank { revision, entries })
    }
}

/// `POST /funding/deposits/check`: the wallet's deposit address, which the
/// relayer now watches.
pub fn deposit_address(text: &str) -> Result<Address, BadWire> {
    let json = parse(text, "deposit_check")?;
    address_of(field(&json, "proxy")?, "proxy")
}

#[cfg(test)]
#[path = "tests/stats.rs"]
mod tests;
