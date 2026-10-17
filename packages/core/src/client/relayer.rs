//! Relayer requests as pure values ([`Call`]) and a client that sends them over
//! [`df_hyperliquid_core::client::Http`]. Submissions are never retried: without an answer the intent may still
//! have been accepted, and the account stream shows whether it was.

use df_hyperliquid_core::utils::{Address, Json, hex};

use df_hyperliquid_core::client::{Http, Method, Request, TransportError};

use crate::client::deployment::ORIGIN;

use crate::trade::intent::Signed;

use crate::utils::wire::{BadWire, field, hash_of, parse, str_of, u128_of};

/// One relayer request, without a base URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: Method,
    /// Path from the root, such as `/state/trading`.
    pub path: String,
    pub body: Option<String>,
}

impl Call {
    fn get(path: impl Into<String>) -> Call {
        Call { method: Method::Get, path: path.into(), body: None }
    }

    /// Submits a signed intent (`POST /intents`, or its cancel path).
    #[must_use]
    pub fn submit(signed: &Signed) -> Call {
        Call { method: Method::Post, path: signed.path(), body: Some(signed.body()) }
    }

    /// The trading state.
    #[must_use]
    pub fn trading_state() -> Call {
        Call::get("/state/trading")
    }

    /// The wallet's account stream. It is server-sent events, so read the body as it arrives.
    #[must_use]
    pub fn account_stream(wallet: Address) -> Call {
        Call::get(format!("/state/user/live?wallet={}", wallet.to_hex()))
    }

    #[must_use]
    pub fn summary() -> Call {
        Call::get("/query/protocol/summary")
    }

    /// Each market's 24 hour volume.
    #[must_use]
    pub fn market_stats() -> Call {
        Call::get("/query/protocol/market-stats")
    }

    #[must_use]
    pub fn queue_summary() -> Call {
        Call::get("/query/protocol/queue-summary")
    }

    #[must_use]
    pub fn recent_trades() -> Call {
        Call::get("/query/protocol/trades/recent")
    }

    /// Where the wallet's queued intents stand in the relayer's queue.
    #[must_use]
    pub fn queue_rank(wallet: Address) -> Call {
        Call::get(format!("/queue-rank/accounts/{}", wallet.to_hex()))
    }

    /// The wallet's deposit address. Asking also registers it with the sweeper, so ask before
    /// depositing.
    #[must_use]
    pub fn deposit_check(wallet: Address) -> Call {
        let body = Json::Object(vec![("user".to_owned(), Json::String(wallet.to_hex()))]).compact();
        Call { method: Method::Post, path: "/funding/deposits/check".to_owned(), body: Some(body) }
    }

    #[must_use]
    pub fn health() -> Call {
        Call::get("/relayer/health")
    }

    /// The full request against `base`, such as [`ORIGIN`] or `/api`.
    #[must_use]
    pub fn request(&self, base: &str) -> Request {
        let mut headers = vec![("Accept".to_owned(), "application/json".to_owned())];
        if self.body.is_some() {
            headers.push(("Content-Type".to_owned(), "application/json".to_owned()));
        }
        Request {
            method: self.method,
            url: format!("{}{}", base.trim_end_matches('/'), self.path),
            headers,
            body: self.body.clone().map(String::into_bytes).unwrap_or_default(),
        }
    }
}

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

/// Why a call came back without what was asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelayerError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The relayer refused, with its HTTP status, error code and message.
    #[error("the relayer refused ({status}{}): {}", code.as_deref().map(|code| format!(", {code}")).unwrap_or_default(), message.as_deref().unwrap_or("no reason given"))]
    Refused { status: u16, code: Option<String>, message: Option<String> },
    #[error("the relayer's answer did not read: {0}")]
    Unreadable(#[from] BadWire),
    #[error("the relayer accepted a different intent ({found}) than the one sent ({sent})")]
    OtherIntent { sent: String, found: String },
}

/// Builds a refusal from the body's `error` (or `code`) and `message`.
fn refusal(status: u16, body: &[u8]) -> RelayerError {
    let json = std::str::from_utf8(body).ok().and_then(|text| Json::parse(text).ok());
    let text = |key: &str| json.as_ref().and_then(|json| json.get(key)).and_then(Json::as_str).map(str::to_owned);
    RelayerError::Refused { status, code: text("error").or_else(|| text("code")), message: text("message") }
}

/// The relayer at one base URL.
#[derive(Debug, Clone)]
pub struct Relayer<H> {
    http: H,
    base: String,
}

impl<H> Relayer<H> {
    /// The relayer at [`ORIGIN`].
    pub fn new(http: H) -> Self {
        Relayer { http, base: ORIGIN.to_owned() }
    }

    /// The relayer at another base URL, such as a proxy at `/api`.
    pub fn at(http: H, base: impl Into<String>) -> Self {
        Relayer { http, base: base.into() }
    }

    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        &self.http
    }
}

impl<H: Http> Relayer<H> {
    /// Sends `call` and returns the body of a 2xx answer as text.
    pub async fn send(&self, call: &Call) -> Result<String, RelayerError> {
        let response = self.http.send(call.request(&self.base)).await?;
        if !(200..300).contains(&response.status) {
            return Err(refusal(response.status, &response.body));
        }
        let text = String::from_utf8(response.body)
            .map_err(|_| BadWire { what: "body".to_owned(), found: "not UTF-8".to_owned() })?;
        // A 2xx can still carry a refusal: `{"ok":false}` or an `error` field.
        if let Ok(json) = Json::parse(&text)
            && (json.get("error").is_some() || matches!(json.get("ok"), Some(Json::Bool(false))))
        {
            return Err(refusal(response.status, text.as_bytes()));
        }
        Ok(text)
    }

    /// Submits a signed intent once and checks the relayer accepted the same intent id. A
    /// cancel's answer is not checked.
    pub async fn submit(&self, signed: &Signed) -> Result<Accepted, RelayerError> {
        let accepted = Accepted::parse(&self.send(&Call::submit(signed)).await?)?;
        if matches!(signed.intent(), crate::trade::intent::Intent::CancelIntent(_)) {
            return Ok(accepted);
        }
        let sent = signed.intent_id();
        if accepted.intent_id != sent {
            return Err(RelayerError::OtherIntent {
                sent: hex::encode_prefixed(&sent),
                found: hex::encode_prefixed(&accepted.intent_id),
            });
        }
        Ok(accepted)
    }

    pub async fn trading_state(&self) -> Result<crate::markets::trading::TradingState, RelayerError> {
        Ok(crate::markets::trading::TradingState::parse(&self.send(&Call::trading_state()).await?)?)
    }

    pub async fn summary(&self) -> Result<crate::markets::stats::Summary, RelayerError> {
        Ok(crate::markets::stats::Summary::parse(&self.send(&Call::summary()).await?)?)
    }

    pub async fn recent_trades(&self) -> Result<Vec<crate::markets::stats::Trade>, RelayerError> {
        Ok(crate::markets::stats::recent_trades(&self.send(&Call::recent_trades()).await?)?)
    }

    /// Fetches the wallet's deposit address, registering it with the sweeper.
    pub async fn deposit_address(&self, wallet: Address) -> Result<Address, RelayerError> {
        Ok(crate::markets::stats::deposit_address(&self.send(&Call::deposit_check(wallet)).await?)?)
    }
}

#[cfg(test)]
#[path = "tests/relayer.rs"]
mod tests;
