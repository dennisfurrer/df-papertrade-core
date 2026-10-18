//! The account's state fields as a frame carries them: indexing progress, registered session
//! keys, balances, funds, the queue's figures and PAPER. A snapshot has all of them, a patch any
//! subset.

use df_hyperliquid_core::utils::Json;

use super::{Account, AccountError, Funds, Paper};
use crate::utils::wire::{BadWire, address_of, array_of, field, tuple, u64_of, u128_of};

impl Account {
    /// Reads the state fields in `json`. With `partial` set, a missing field is left as it was.
    pub(super) fn fields(&mut self, json: &Json, partial: bool) -> Result<(), AccountError> {
        let get = |key: &str| -> Result<Option<&Json>, BadWire> {
            match json.get(key) {
                Some(value) => Ok(Some(value)),
                None if partial => Ok(None),
                None => field(json, key).map(Some),
            }
        };
        if let Some(value) = get("indexedThroughBlock")? {
            self.indexed_through_block = u64_of(value, "wallet_block")?;
        }
        if let Some(value) = get("walletRevision")? {
            self.wallet_revision = u64_of(value, "wallet_revision")?;
        }
        if let Some(value) = get("indexedThroughTimestamp")? {
            self.indexed_through_timestamp = u64_of(value, "wallet_timestamp")?;
        }
        if let Some(value) = get("sessionKeys")? {
            self.session_keys = array_of(value, "wallet_session_keys")?
                .iter()
                .map(|row| {
                    let f = tuple(row, 2, "wallet_session_keys")?;
                    Ok((address_of(&f[0], "wallet_session_keys")?, u64_of(&f[1], "wallet_session_expiry")?))
                })
                .collect::<Result<_, BadWire>>()?;
        }
        if let Some(value) = get("queueRevision")? {
            self.queue_revision = u128_of(value, "queue_revision")?;
        }
        if let Some(value) = get("balance")? {
            self.balance = u128_of(value, "wallet_balance")?;
        }
        if let Some(value) = get("queued")? {
            self.queued = u128_of(value, "wallet_queued")?;
        }
        if let Some(value) = get("locked")? {
            self.locked = u128_of(value, "wallet_locked")?;
        }
        if let Some(value) = get("funds")? {
            self.funds = Funds::read(value)?;
        }
        if let Some(value) = get("queueNodeCount")? {
            self.queue_node_count = u64_of(value, "wallet_queue_count")?;
        }
        if let Some(value) = get("consumableDebt")? {
            self.consumable_debt = u128_of(value, "wallet_consumable_debt")?;
        }
        if let Some(value) = get("paper")? {
            self.paper = Paper::read(value)?;
        }
        Ok(())
    }
}
