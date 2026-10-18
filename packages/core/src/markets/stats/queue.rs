//! `GET /query/protocol/queue-summary`: the payout queue, what every queued winner is owed and
//! its largest and front entries.

use df_hyperliquid_core::utils::Json;

use crate::utils::wire::{BadWire, field, parse, u64_of, u128_of};

/// `GET /query/protocol/queue-summary`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueSummary {
    pub block: u64,
    /// Owed to every queued winner.
    pub total: u128,
    pub count: u64,
    /// The largest and front entries, as raw JSON.
    pub largest: Option<Json>,
    pub front: Option<Json>,
}

impl QueueSummary {
    pub fn parse(text: &str) -> Result<QueueSummary, BadWire> {
        let json = parse(text, "queue_summary")?;
        let present = |key: &str| json.get(key).filter(|value| !matches!(value, Json::Null)).cloned();
        Ok(QueueSummary {
            block: u64_of(field(field(&json, "source")?, "block")?, "block")?,
            total: u128_of(field(&json, "total")?, "total")?,
            count: u64_of(field(&json, "count")?, "count")?,
            largest: present("largest"),
            front: present("front"),
        })
    }
}

#[cfg(test)]
#[path = "tests/queue.rs"]
mod tests;
