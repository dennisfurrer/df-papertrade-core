//! A patch frame against a named revision: changed state fields, the closes and intent outcomes
//! it reports, and the positions and operations it removes and adds. Removing what is not held
//! or adding what already is means the stream is out of sync.

use df_hyperliquid_core::utils::Json;

use super::closed::read_outcome;
use super::{Account, AccountError, ClosedPosition, Lifetime, Operation, Position};
use crate::utils::wire::{array_of, field, hash_of, u64_of, u128_of};

impl Account {
    /// The account after `json`, a patch against this account's revision.
    pub(super) fn patch(&self, json: &Json) -> Result<Account, AccountError> {
        let base = u64_of(field(json, "baseAccountRevision")?, "account_revision")?;
        if base != self.account_revision {
            return Err(AccountError::OutOfSync("the patch is for another revision"));
        }
        let mut next = self.clone();
        next.account_revision = u64_of(field(json, "accountRevision")?, "account_revision")?;
        if next.account_revision <= self.account_revision {
            return Err(AccountError::OutOfSync("the patch does not move the revision on"));
        }
        if let Some(state) = json.get("state") {
            next.fields(state, true)?;
        }
        if let Some(lifetime) = json.get("lifetime") {
            next.lifetime = Lifetime::read(lifetime)?;
        }
        next.closed = match json.get("closedPositions") {
            Some(rows) => {
                array_of(rows, "closed_positions")?.iter().map(ClosedPosition::read).collect::<Result<_, _>>()?
            }
            None => Vec::new(),
        };
        next.outcomes = match json.get("intentOutcomes") {
            Some(rows) => array_of(rows, "intent_outcomes")?.iter().map(read_outcome).collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        next.patch_positions(json)?;
        next.patch_operations(json)?;
        next.check()?;
        Ok(next)
    }

    /// Removes and adds positions, keeping them newest first.
    fn patch_positions(&mut self, json: &Json) -> Result<(), AccountError> {
        let removed = array_of(field(json, "removedPositionIds")?, "wallet_patch")?
            .iter()
            .map(|id| u128_of(id, "removed_position_id"))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, id) in removed.iter().enumerate() {
            if removed[..index].contains(id) || !self.positions.iter().any(|position| position.position_id == *id) {
                return Err(AccountError::OutOfSync("removes a position it does not hold"));
            }
        }
        self.positions.retain(|position| !removed.contains(&position.position_id));
        for row in array_of(field(json, "addedPositions")?, "wallet_patch")? {
            let position = Position::read(row)?;
            if removed.contains(&position.position_id)
                || self.positions.iter().any(|held| held.position_id == position.position_id)
            {
                return Err(AccountError::OutOfSync("adds a position it already holds"));
            }
            self.positions.push(position);
        }
        self.positions.sort_by(|a, b| b.position_id.cmp(&a.position_id));
        Ok(())
    }

    /// Removes and adds operations by intent id.
    fn patch_operations(&mut self, json: &Json) -> Result<(), AccountError> {
        let removed = array_of(field(json, "removedOperationIds")?, "wallet_patch")?
            .iter()
            .map(|id| hash_of(id, "operation_intent"))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, id) in removed.iter().enumerate() {
            if removed[..index].contains(id) || !self.operations.iter().any(|operation| operation.intent_id == *id) {
                return Err(AccountError::OutOfSync("removes an operation it does not hold"));
            }
        }
        self.operations.retain(|operation| !removed.contains(&operation.intent_id));
        for row in array_of(field(json, "addedOperations")?, "wallet_patch")? {
            let operation = Operation::read(row)?;
            if removed.contains(&operation.intent_id)
                || self.operations.iter().any(|held| held.intent_id == operation.intent_id)
            {
                return Err(AccountError::OutOfSync("adds an operation it already holds"));
            }
            self.operations.push(operation);
        }
        Ok(())
    }
}
