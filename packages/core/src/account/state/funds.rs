//! The wallet's money outside its positions: funds reserved for accepted intents and what
//! remains available, its PAPER and staking position, and its totals since the first deposit.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, field, i128_of, u128_of};

/// Funds reserved for accepted intents, and what remains available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Funds {
    pub reserved_balance: u128,
    pub reserved_queued: u128,
    /// What a new open from the balance, or a withdrawal, may use.
    pub available_balance: u128,
    /// What a new open from the queued balance may use.
    pub available_queued: u128,
}

impl Funds {
    pub(super) fn read(json: &Json) -> Result<Funds, BadWire> {
        Ok(Funds {
            reserved_balance: u128_of(field(json, "reservedBalance")?, "funds_reservedBalance")?,
            reserved_queued: u128_of(field(json, "reservedQueued")?, "funds_reservedQueued")?,
            available_balance: u128_of(field(json, "availableBalance")?, "funds_availableBalance")?,
            available_queued: u128_of(field(json, "availableQueued")?, "funds_availableQueued")?,
        })
    }
}

/// The wallet's PAPER and its staking position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Paper {
    pub balance: u128,
    pub staked: u128,
    pub pending_base: u128,
    pub reward_debt: u128,
}

impl Paper {
    /// Claimable USDC rewards in collateral units, given the staking accumulator
    /// `acc_reward_per_share` (scaled by 10^27).
    #[must_use]
    pub fn pending_reward(&self, acc_reward_per_share: u128) -> u128 {
        let earned =
            crate::trade::math::mul_div(self.staked, acc_reward_per_share, 1_000_000_000_000_000_000_000_000_000);
        let fresh = earned.map_or(0, |earned| earned.saturating_sub(self.reward_debt));
        self.pending_base.saturating_add(fresh)
    }

    pub(super) fn read(json: &Json) -> Result<Paper, BadWire> {
        Ok(Paper {
            balance: u128_of(field(json, "balance")?, "paper_balance")?,
            staked: u128_of(field(json, "staked")?, "paper_staked")?,
            pending_base: u128_of(field(json, "pendingBase")?, "paper_pending")?,
            reward_debt: u128_of(field(json, "rewardDebt")?, "paper_debt")?,
        })
    }
}

/// Totals since the wallet's first deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Lifetime {
    pub deposited: u128,
    pub withdrawn: u128,
    pub realized_pnl: i128,
    pub fees_paid: u128,
    pub traded_notional: u128,
    pub paper_rewards_claimed: u128,
}

impl Lifetime {
    pub(super) fn read(json: &Json) -> Result<Lifetime, BadWire> {
        Ok(Lifetime {
            deposited: u128_of(field(json, "depositedRaw")?, "lifetime_deposited")?,
            withdrawn: u128_of(field(json, "withdrawnRaw")?, "lifetime_withdrawn")?,
            realized_pnl: i128_of(field(json, "realizedPnlRaw")?, "lifetime_realized")?,
            fees_paid: u128_of(field(json, "feesPaidRaw")?, "lifetime_fees")?,
            traded_notional: u128_of(field(json, "tradedNotionalRaw")?, "lifetime_notional")?,
            paper_rewards_claimed: u128_of(field(json, "paperRewardsClaimedRaw")?, "lifetime_paper")?,
        })
    }
}

#[cfg(test)]
#[path = "tests/funds.rs"]
mod tests;
