//! Papertrade's integer math, following the Exchange's `QueueLib`: bust prices, PnL after
//! the payout curve, impact curve, win fee and builder fee, open limits and the PAPER mint rate. Every division rounds
//! toward zero, and wide products go through [`df_hyperliquid_core::client::evm::U256`].

pub mod bust;
pub mod limits;
pub mod mint;
pub mod valuation;

pub use bust::bust_price;
pub use limits::{Limit, OpenLimits, OpenRefusal, check_open, debt_room, open_limits};
pub use mint::{mint_rate, paper_for_loss};
pub use valuation::{BUILDER_FEE_RATE, Curve, DEADBAND_DIVISOR, Rates, Valuation, ValuationError, WIN_FEE_RATE, value};

use df_hyperliquid_core::client::evm::U256;

/// `a * b / c` with a 256-bit intermediate, rounded toward zero. `None` for a zero divisor
/// or a result past 128 bits.
#[must_use]
pub fn mul_div(a: u128, b: u128, c: u128) -> Option<u128> {
    let (quotient, _) = U256::from(a).checked_mul(U256::from(b))?.div_rem(U256::from(c))?;
    quotient.to_u128()
}
