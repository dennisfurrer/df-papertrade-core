//! A snapshot frame: the whole account at one revision, with its operations, its open positions
//! newest first, its own open interest caps and the latest nonces the relayer has seen. A
//! snapshot without `oiCaps` or `recentNonces`, as the relayer sent before 2026-11, reads as none.

use df_hyperliquid_core::utils::Json;

use super::{Account, AccountError, Funds, Lifetime, OiCap, Operation, Paper, Position, RecentNonces};
use crate::utils::wire::{address_of, array_of, field, u64_of};

impl Account {
    /// Reads a snapshot. Every state field must be there.
    pub(super) fn snapshot(json: &Json) -> Result<Account, AccountError> {
        let mut account = Account {
            address: address_of(field(json, "address")?, "wallet_identity")?,
            account_revision: u64_of(field(json, "accountRevision")?, "account_revision")?,
            indexed_through_block: 0,
            wallet_revision: 0,
            indexed_through_timestamp: 0,
            session_keys: Vec::new(),
            queue_revision: 0,
            balance: 0,
            queued: 0,
            locked: 0,
            funds: Funds::default(),
            queue_node_count: 0,
            consumable_debt: 0,
            paper: Paper::default(),
            lifetime: Lifetime::read(field(json, "lifetime")?)?,
            operations: array_of(field(json, "operations")?, "operations")?
                .iter()
                .map(Operation::read)
                .collect::<Result<_, _>>()?,
            positions: array_of(field(json, "openPositions")?, "positions")?
                .iter()
                .map(Position::read)
                .collect::<Result<_, _>>()?,
            closed: Vec::new(),
            outcomes: Vec::new(),
            oi_caps: json.get("oiCaps").map(OiCap::read_all).transpose()?.unwrap_or_default(),
            recent_nonces: json.get("recentNonces").map(RecentNonces::read).transpose()?.unwrap_or_default(),
        };
        account.fields(json, false)?;
        let descending = account.positions.windows(2).all(|pair| pair[0].position_id > pair[1].position_id);
        if !descending {
            return Err(AccountError::OutOfSync("positions out of order"));
        }
        account.check()?;
        Ok(account)
    }
}
