//! An open position as the account stream lists it: a 10-field row with its side, margin,
//! leverage, entry and bust prices.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, bad, bool_of, hash_of, nullable, tuple, u32_of, u64_of, u128_of};

/// An open position. Amounts are collateral units (18 decimals), prices are raw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub position_id: u128,
    pub instrument_id: u32,
    pub is_long: bool,
    /// Opened from the queued balance.
    pub use_debt: bool,
    pub entry_price: u128,
    pub margin: u128,
    pub leverage: u32,
    /// Price at which it is liquidated.
    pub bust_price: u128,
    pub opened_at_ms: u64,
    /// The intent that opened it. `None` for positions older than intent ids.
    pub intent_id: Option<[u8; 32]>,
}

impl Position {
    /// Margin x leverage, in collateral units.
    #[must_use]
    pub fn notional(&self) -> u128 {
        self.margin.saturating_mul(u128::from(self.leverage))
    }

    pub(super) fn read(row: &Json) -> Result<Position, BadWire> {
        let f = tuple(row, 10, "position_tuple")?;
        let position = Position {
            position_id: u128_of(&f[0], "position_id")?,
            instrument_id: u32_of(&f[1], "position_instrument")?,
            is_long: bool_of(&f[2], "position_side")?,
            use_debt: bool_of(&f[3], "position_debt")?,
            entry_price: u128_of(&f[4], "position_entry")?,
            margin: u128_of(&f[5], "position_margin")?,
            leverage: u32_of(&f[6], "position_leverage")?,
            bust_price: u128_of(&f[7], "position_bust")?,
            opened_at_ms: u64_of(&f[8], "position_opened_at")?,
            intent_id: nullable(&f[9], "position_intent", hash_of)?,
        };
        if position.entry_price == 0 || position.leverage == 0 {
            return Err(bad("position_tuple", row));
        }
        Ok(position)
    }
}
