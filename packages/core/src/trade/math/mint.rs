//! The PAPER mint: how much PAPER a dollar of loss mints, flat below the cliff and decaying on
//! the tail past it.

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

/// PAPER (18 decimals) a loss of `loss` collateral units mints at `rate`.
#[must_use]
pub fn paper_for_loss(loss: u128, rate: u128) -> u128 {
    mul_div(loss, rate, WAD).unwrap_or(0)
}

#[cfg(test)]
#[path = "tests/mint.rs"]
mod tests;
