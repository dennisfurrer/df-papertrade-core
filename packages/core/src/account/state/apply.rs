//! Folding the stream's `wallet` frames into an [`Account`]: a frame for another wallet is
//! refused, a snapshot replaces the account unless it is older, a patch applies to the revision
//! it names, and every result is checked to add up.

use df_hyperliquid_core::utils::Address;

use super::{Account, AccountError, Applied};
use crate::utils::wire::{address_of, field, parse};

impl Account {
    /// Parses a single snapshot frame.
    pub fn parse(text: &str) -> Result<Account, AccountError> {
        let json = parse(text, "wallet_frame")?;
        if json.get("baseAccountRevision").is_some() {
            return Err(AccountError::OutOfSync("a patch with no snapshot before it"));
        }
        Account::snapshot(&json)
    }

    /// Folds in a `wallet` event's data. A snapshot replaces the account, a patch
    /// applies to the revision it names.
    pub fn apply(held: Option<&Account>, text: &str, wallet: Address) -> Result<(Account, Applied), AccountError> {
        let json = parse(text, "wallet_frame")?;
        let found = address_of(field(&json, "address")?, "wallet_identity")?;
        if found != wallet {
            return Err(AccountError::OtherWallet { expected: wallet, found });
        }
        if json.get("baseAccountRevision").is_none() {
            let account = Account::snapshot(&json)?;
            if let Some(held) = held
                && account.account_revision <= held.account_revision
            {
                return Ok((held.clone(), Applied::Stale));
            }
            return Ok((account, Applied::Snapshot));
        }
        let Some(held) = held else {
            return Err(AccountError::OutOfSync("a patch with no snapshot before it"));
        };
        Ok((held.patch(&json)?, Applied::Patch))
    }

    /// Consistency checks run on every frame.
    pub(super) fn check(&self) -> Result<(), AccountError> {
        if self.wallet_revision > self.indexed_through_block {
            return Err(AccountError::OutOfSync("the wallet revision is past the indexed block"));
        }
        let locked = self.positions.iter().try_fold(0u128, |sum, position| sum.checked_add(position.margin));
        if locked != Some(self.locked) {
            return Err(AccountError::OutOfSync("locked margin is not the positions' margin"));
        }
        let available_balance = self.balance.saturating_sub(self.funds.reserved_balance);
        let available_queued = self.queued.saturating_sub(self.funds.reserved_queued);
        if self.funds.available_balance != available_balance || self.funds.available_queued != available_queued {
            return Err(AccountError::OutOfSync("available funds do not add up"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/apply.rs"]
mod tests;
