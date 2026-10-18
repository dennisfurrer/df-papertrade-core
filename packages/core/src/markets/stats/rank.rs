//! `GET /queue-rank/accounts/<wallet>`: where each of a wallet's queued intents stands in the
//! relayer's queue.

use crate::utils::wire::{BadWire, array_of, field, hash_of, parse, u64_of, u128_of};

/// `GET /queue-rank/accounts/<wallet>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRank {
    pub revision: Option<u128>,
    /// Each queued intent of the wallet's and its place in the relayer's queue.
    pub entries: Vec<([u8; 32], u64)>,
}

impl QueueRank {
    pub fn parse(text: &str) -> Result<QueueRank, BadWire> {
        let json = parse(text, "queue_rank")?;
        let revision = match json.get("queueRevision") {
            Some(value) => Some(u128_of(value, "queueRevision")?),
            None => None,
        };
        let entries = array_of(field(&json, "entries")?, "entries")?
            .iter()
            .map(|entry| {
                Ok((hash_of(field(entry, "intentId")?, "intentId")?, u64_of(field(entry, "position")?, "position")?))
            })
            .collect::<Result<_, BadWire>>()?;
        Ok(QueueRank { revision, entries })
    }
}

#[cfg(test)]
#[path = "tests/rank.rs"]
mod tests;
