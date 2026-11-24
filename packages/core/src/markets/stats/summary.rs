//! `GET /query/protocol/summary`: what the Exchange holds, its activity and fees, and PAPER's
//! supply, staking and mint curve (contract set 1's figures, and contract set 2's constant).

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
    /// `rebateC36`: contract set 2's mint constant `c`, PAPER minted as a share of the supply per
    /// dollar of loss basis, times `10^36` (`floor(c x 10^36)`, as PaperTokenomics'
    /// `rebateC36()` returns it). About `1.101 x 10^27` on 2026-11-24, `c` about `1.101 x 10^-9`
    /// a dollar. `None` when not sent, as before contract set 2.
    pub rebate_c36: Option<u128>,
}

impl PaperFigures {
    /// PAPER per dollar of loss now, 18 decimals: contract set 2's rate
    /// ([`rebate_rate`](crate::trade::math::rebate_rate), without the insolvency bonus) when
    /// [`PaperFigures::rebate_c36`] is sent, else contract set 1's curve.
    #[must_use]
    pub fn mint_rate(&self) -> u128 {
        match self.rebate_c36 {
            Some(c36) => crate::trade::math::rebate_rate(self.supply, c36),
            None => {
                crate::trade::math::mint_rate(self.tracked_lp, self.cliff, self.rate, self.decay, self.tail_progress)
            }
        }
    }

    /// PAPER (18 decimals) a loss basis of `basis` collateral units mints now, contract set 2's
    /// rule ([`rebate_mint`](crate::trade::math::rebate_mint)) when [`PaperFigures::rebate_c36`]
    /// is sent, else contract set 1's rate times the loss.
    #[must_use]
    pub fn mint_for_loss(&self, basis: u128) -> u128 {
        match self.rebate_c36 {
            Some(c36) => crate::trade::math::rebate_mint(basis, self.supply, c36),
            None => crate::trade::math::paper_for_loss(basis, self.mint_rate()),
        }
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
                rebate_c36: paper
                    .get("rebateC36")
                    .filter(|value| **value != Json::Null)
                    .map(|value| u128_of(value, "rebateC36"))
                    .transpose()?,
            },
        })
    }
}

#[cfg(test)]
#[path = "tests/summary.rs"]
mod tests;
