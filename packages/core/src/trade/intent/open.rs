//! Opening a position: its market, side, margin and leverage, and whether the margin comes from
//! the queued balance.

use df_hyperliquid_core::utils::Address;

use super::Refusal;

/// Highest leverage any market allows. A market's own cap may be lower.
pub const MAX_LEVERAGE: u32 = 1000;

/// Opens a position. `size` is the margin in collateral units (18 decimals), drawn from the
/// queued balance when `use_debt` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPosition {
    pub user: Address,
    pub instrument_id: u32,
    pub is_long: bool,
    pub size: u128,
    pub leverage: u32,
    pub use_debt: bool,
    pub nonce: u128,
    pub deadline: u64,
}

impl OpenPosition {
    /// Refuses leverage outside 1 to [`MAX_LEVERAGE`] and a zero margin.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user: Address,
        instrument_id: u32,
        is_long: bool,
        size: u128,
        leverage: u32,
        use_debt: bool,
        nonce: u128,
        deadline: u64,
    ) -> Result<Self, Refusal> {
        if leverage == 0 || leverage > MAX_LEVERAGE {
            return Err(Refusal::Leverage(leverage));
        }
        if size == 0 {
            return Err(Refusal::NoMargin);
        }
        Ok(OpenPosition { user, instrument_id, is_long, size, leverage, use_debt, nonce, deadline })
    }
}
