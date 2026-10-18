//! The bust price: where a position is liquidated, a buffer inside the price that would wipe
//! out its margin.

use super::mul_div;
use crate::utils::units::WAD;

/// Bust price for a raw `entry` and `leverage`: the entry moved `1 / leverage` against the
/// position, then `bust_buffer` back toward it.
#[must_use]
pub fn bust_price(entry: u128, leverage: u32, is_long: bool, bust_buffer: u128) -> Option<u128> {
    if leverage == 0 {
        return None;
    }
    let step = entry / u128::from(leverage);
    if is_long {
        let zero = entry.checked_sub(step)?;
        zero.checked_add(mul_div(zero, bust_buffer, WAD)?)
    } else {
        let zero = entry.checked_add(step)?;
        zero.checked_sub(mul_div(zero, bust_buffer, WAD)?)
    }
}

#[cfg(test)]
#[path = "tests/bust.rs"]
mod tests;
