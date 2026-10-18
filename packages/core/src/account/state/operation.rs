//! The intents the relayer has accepted for the wallet and not yet settled, read from the
//! stream's 10-field operation rows.

use df_hyperliquid_core::utils::Json;

use crate::trade::intent::MAX_CLOSE_IDS;
use crate::utils::units::COLLATERAL_PER_USDC;
use crate::utils::wire::{BadWire, array_of, bad, bool_of, hash_of, nullable, str_of, tuple, u32_of, u64_of, u128_of};

/// What a queued operation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationKind {
    OpenPosition {
        instrument_id: u32,
        is_long: bool,
        use_debt: bool,
        margin: u128,
        leverage: u32,
    },
    ClosePositions {
        position_ids: Vec<u128>,
    },
    /// `amount` in collateral units, scaled up from the intent's USDC (6 decimals).
    WithdrawToCore {
        amount: u128,
    },
    RegisterSessionKey,
    /// PAPER, 18 decimals.
    PaperStake {
        amount: u128,
    },
    PaperUnstake {
        amount: u128,
    },
    PaperClaim,
    PaperDistribute,
}

impl OperationKind {
    /// The relayer's action name.
    #[must_use]
    pub const fn action(&self) -> &'static str {
        match self {
            OperationKind::OpenPosition { .. } => "openPosition",
            OperationKind::ClosePositions { .. } => "closePositions",
            OperationKind::WithdrawToCore { .. } => "withdrawToCore",
            OperationKind::RegisterSessionKey => "registerSessionKey",
            OperationKind::PaperStake { .. } => "paperStake",
            OperationKind::PaperUnstake { .. } => "paperUnstake",
            OperationKind::PaperClaim => "paperClaim",
            OperationKind::PaperDistribute => "paperDistribute",
        }
    }
}

/// An intent the relayer has accepted and not yet settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub intent_id: [u8; 32],
    pub kind: OperationKind,
    pub accepted_at_ms: u64,
    /// Unix seconds after which it expires unsent.
    pub deadline: u64,
}

impl Operation {
    pub(super) fn read(row: &Json) -> Result<Operation, BadWire> {
        let f = tuple(row, 10, "operation_tuple")?;
        let action = str_of(&f[1], "operation_action")?;
        let instrument = nullable(&f[2], "operation_instrument", u32_of)?;
        let is_long = nullable(&f[3], "operation_open", bool_of)?;
        let use_debt = nullable(&f[4], "operation_open", bool_of)?;
        let amount = nullable(&f[5], "operation_amount", u128_of)?;
        let leverage = nullable(&f[6], "operation_leverage", u32_of)?;
        let close_ids = match &f[9] {
            Json::Null => None,
            ids => {
                let ids = array_of(ids, "operation_close_ids")?
                    .iter()
                    .map(|id| u128_of(id, "operation_close_id"))
                    .collect::<Result<Vec<_>, _>>()?;
                let distinct = ids.iter().enumerate().all(|(index, id)| !ids[..index].contains(id));
                if ids.is_empty() || ids.len() > MAX_CLOSE_IDS || !distinct {
                    return Err(bad("operation_close_ids", &f[9]));
                }
                Some(ids)
            }
        };
        if action != "closePositions" && close_ids.is_some() {
            return Err(bad("operation_close_ids", &f[9]));
        }
        let amount_of = |what| amount.ok_or_else(|| bad(what, &f[5]));
        let kind = match action {
            "openPosition" => match (instrument, is_long, use_debt, amount, leverage) {
                (Some(instrument_id), Some(is_long), Some(use_debt), Some(margin), Some(leverage)) if leverage > 0 => {
                    OperationKind::OpenPosition { instrument_id, is_long, use_debt, margin, leverage }
                }
                _ => return Err(bad("operation_open", row)),
            },
            "closePositions" => OperationKind::ClosePositions {
                position_ids: close_ids.ok_or_else(|| bad("operation_close_ids", row))?,
            },
            "withdrawToCore" => OperationKind::WithdrawToCore {
                amount: amount_of("operation_amount")?.saturating_mul(COLLATERAL_PER_USDC),
            },
            "paperStake" => OperationKind::PaperStake { amount: amount_of("operation_amount")? },
            "paperUnstake" => OperationKind::PaperUnstake { amount: amount_of("operation_amount")? },
            "registerSessionKey" => OperationKind::RegisterSessionKey,
            "paperClaim" => OperationKind::PaperClaim,
            "paperDistribute" => OperationKind::PaperDistribute,
            _ => return Err(bad("operation_action", &f[1])),
        };
        Ok(Operation {
            intent_id: hash_of(&f[0], "operation_intent")?,
            kind,
            accepted_at_ms: u64_of(&f[7], "operation_time")?,
            deadline: u64_of(&f[8], "operation_deadline")?,
        })
    }
}
