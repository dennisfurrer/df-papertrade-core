//! What a patch reports as ended: positions closed or liquidated, and how intents ended.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, bad, bool_of, hash_of, i128_of, str_of, tuple, u32_of, u128_of};

/// A position a patch reports closed or liquidated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedPosition {
    pub position_id: u128,
    pub instrument_id: u32,
    pub is_long: bool,
    pub margin: u128,
    pub leverage: u32,
    pub liquidated: bool,
    /// PnL after impact and fees, negative for a loss.
    pub net_pnl: i128,
    /// PAPER minted for the loss, 18 decimals.
    pub paper_minted: u128,
}

impl ClosedPosition {
    pub(super) fn read(row: &Json) -> Result<ClosedPosition, BadWire> {
        let f = tuple(row, 8, "closed_position_tuple")?;
        Ok(ClosedPosition {
            position_id: u128_of(&f[0], "closed_position_id")?,
            instrument_id: u32_of(&f[1], "closed_position_instrument")?,
            is_long: bool_of(&f[2], "closed_position_side")?,
            margin: u128_of(&f[3], "closed_position_margin")?,
            leverage: u32_of(&f[4], "closed_position_leverage")?,
            liquidated: bool_of(&f[5], "closed_position_kind")?,
            net_pnl: i128_of(&f[6], "closed_position_pnl")?,
            paper_minted: u128_of(&f[7], "closed_position_paper")?,
        })
    }
}

/// How an intent ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    Expired,
    Cancelled,
    Unknown,
}

pub(super) fn read_outcome(row: &Json) -> Result<([u8; 32], Outcome), BadWire> {
    let f = tuple(row, 2, "intent_outcome_tuple")?;
    let outcome = match str_of(&f[1], "intent_outcome")? {
        "succeeded" => Outcome::Succeeded,
        "failed" => Outcome::Failed,
        "expired" => Outcome::Expired,
        "cancelled" => Outcome::Cancelled,
        "unknown" => Outcome::Unknown,
        _ => return Err(bad("intent_outcome", &f[1])),
    };
    Ok((hash_of(&f[0], "intent_outcome")?, outcome))
}

#[cfg(test)]
#[path = "tests/closed.rs"]
mod tests;
