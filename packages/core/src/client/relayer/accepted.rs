//! What the relayer answers a submission or cancel it took: the intent id, its action and state,
//! and the queue's revision.

use crate::utils::wire::{BadWire, field, hash_of, parse, str_of, u128_of};

/// What the relayer answers an accepted submission or cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub intent_id: [u8; 32],
    pub action: String,
    /// Such as `queued`, `prepared`, `confirmed` or `cancelled`.
    pub state: String,
    /// The queue's revision once it took the intent. `None` for a cancel.
    pub queue_revision: Option<u128>,
}

impl Accepted {
    pub fn parse(text: &str) -> Result<Accepted, BadWire> {
        let json = parse(text, "intent_answer")?;
        Ok(Accepted {
            intent_id: hash_of(field(&json, "intentId")?, "intentId")?,
            action: str_of(field(&json, "action")?, "action")?.to_owned(),
            state: str_of(field(&json, "state")?, "state")?.to_owned(),
            queue_revision: match json.get("queueRevision") {
                Some(value) => Some(u128_of(value, "queueRevision")?),
                None => None,
            },
        })
    }

    /// Whether the relayer kept it: `queued`, `prepared` or `confirmed`.
    #[must_use]
    pub fn kept(&self) -> bool {
        matches!(self.state.as_str(), "queued" | "prepared" | "confirmed")
    }
}
