//! The PAPER mint: how much PAPER a dollar of loss mints.
//!
//! Contract set 2 mints `supply x (e^(c x g) - 1)` for a loss basis of `g` dollars, with `c` the
//! constant `activateRebate()` fixed (`rebateC36()` returns `floor(c x 10^36)`,
//! [`PaperFigures::rebate_c36`](crate::markets::stats::summary::PaperFigures::rebate_c36)), and
//! counts the part of a loss that refills an underwater LP up to 5 times
//! (`docs.papertrade.xyz/how/mint-curve`). [`rebate_mint`] and [`rebate_rate`] follow it without
//! that bonus. Contract set 1's curve, flat below the cliff and decaying on the tail past it, is
//! [`mint_rate`].

use super::mul_div;
use crate::utils::units::WAD;

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

/// `10^36`, the scale of `rebateC36`.
const C36: u128 = WAD * WAD;

/// The most terms of `e^x - 1` summed. Past `x = 40` (a loss basis of about $36 billion at the
/// live `c`) the sum is cut short and the mint is understated.
const TERMS: u128 = 64;

/// `e^x - 1` for `x` scaled by `10^36`, scaled by `10^36`, rounded toward zero. `None` past
/// 128 bits.
fn exp_minus_one(x: u128) -> Option<u128> {
    let (mut term, mut sum) = (x, x);
    for k in 2..=TERMS {
        term = mul_div(term, x, k.checked_mul(C36)?)?;
        if term == 0 {
            break;
        }
        sum = sum.checked_add(term)?;
    }
    Some(sum)
}

/// PAPER (18 decimals) contract set 2 mints for a loss basis of `basis` collateral units (18
/// decimals, a dollar each `10^18`): `supply x (e^(c x g) - 1)`, with `supply` PAPER's total
/// supply (18 decimals), `c36` the summary's `rebateC36` and `g` the basis in dollars. Without
/// the insolvency bonus. Rounded toward zero, so it may fall short of the contract's figure by a
/// few units. 0 for a basis too large to compute.
#[must_use]
pub fn rebate_mint(basis: u128, supply: u128, c36: u128) -> u128 {
    let Some(exponent) = mul_div(c36, basis, WAD) else { return 0 };
    exp_minus_one(exponent).and_then(|grown| mul_div(supply, grown, C36)).unwrap_or(0)
}

/// PAPER (18 decimals) contract set 2 mints for one dollar of loss basis now: [`rebate_mint`]
/// of `10^18`. As a rate it is what [`paper_for_loss`] takes, close enough for a loss of a few
/// million dollars; past that the curve's growth makes [`rebate_mint`] of the whole loss larger.
#[must_use]
pub fn rebate_rate(supply: u128, c36: u128) -> u128 {
    rebate_mint(WAD, supply, c36)
}

/// PAPER (18 decimals) a loss of `loss` collateral units mints at `rate`.
#[must_use]
pub fn paper_for_loss(loss: u128, rate: u128) -> u128 {
    mul_div(loss, rate, WAD).unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/mint.rs"]
mod tests;
