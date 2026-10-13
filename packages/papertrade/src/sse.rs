//! Server-sent events, sans-IO: hand [`Events::push`] the stream's text as it
//! arrives, in chunks of any size, and take whole events back. The account
//! stream sends `liveness` once (`{"heartbeatIntervalMs":10000}`), then
//! `wallet` frames and a `heartbeat` (`null`) every interval; a stream quiet
//! for longer than a few intervals is dead and is opened again.

/// One event: its name (`message` when none was given) and its data lines
/// joined by newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub data: String,
    pub id: Option<String>,
}

/// The account stream's event names.
pub const LIVENESS: &str = "liveness";
pub const WALLET: &str = "wallet";
pub const HEARTBEAT: &str = "heartbeat";

/// What is left of the stream between chunks.
#[derive(Debug, Clone, Default)]
pub struct Events {
    pending: String,
    name: Option<String>,
    data: Vec<String>,
    id: Option<String>,
}

impl Events {
    #[must_use]
    pub fn new() -> Events {
        Events::default()
    }

    /// Takes the next chunk and returns every event it completed.
    pub fn push(&mut self, chunk: &str) -> Vec<Event> {
        self.pending.push_str(chunk);
        let mut events = Vec::new();
        while let Some(end) = self.pending.find('\n') {
            let line: String = self.pending.drain(..=end).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() || self.name.is_some() {
                    events.push(Event {
                        name: self.name.take().unwrap_or_else(|| "message".to_owned()),
                        data: self.data.join("\n"),
                        id: self.id.take(),
                    });
                    self.data.clear();
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (key, value) = line.split_once(':').unwrap_or((line, ""));
            let value = value.strip_prefix(' ').unwrap_or(value);
            match key {
                "event" => self.name = Some(value.to_owned()),
                "data" => self.data.push(value.to_owned()),
                "id" => self.id = Some(value.to_owned()),
                _ => {}
            }
        }
        events
    }
}

/// The heartbeat interval a `liveness` event names, in milliseconds.
#[must_use]
pub fn heartbeat_ms(event: &Event) -> Option<u64> {
    let json = hl_core::Json::parse(&event.data).ok()?;
    json.get("heartbeatIntervalMs")?.as_u64()
}

#[cfg(test)]
#[path = "tests/sse.rs"]
mod tests;
