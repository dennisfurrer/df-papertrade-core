//! `GET /query/protocol/summary`: what the Exchange holds, its activity and fees, and PAPER's
//! supply, staking and mint curve.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, field, i128_of, parse, u64_of, u128_of};

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
    /// Running LP profit and loss the mint curve reads. Negative while the LP is under water.
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
        crate::trade::math::mint_rate(self.tracked_lp, self.cliff, self.rate, self.decay, self.tail_progress)
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

#[cfg(test)]
#[path = "tests/summary.rs"]
mod tests;
