//! Why a relayer call came back without what was asked: no answer, a refusal with the relayer's
//! code and words, an answer that does not read, or a different intent than the one sent.

use df_hyperliquid_core::client::TransportError;
use df_hyperliquid_core::utils::Json;

use crate::utils::wire::BadWire;

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
pub(super) fn refusal(status: u16, body: &[u8]) -> RelayerError {
    let json = std::str::from_utf8(body).ok().and_then(|text| Json::parse(text).ok());
    let text = |key: &str| json.as_ref().and_then(|json| json.get(key)).and_then(Json::as_str).map(str::to_owned);
    RelayerError::Refused { status, code: text("error").or_else(|| text("code")), message: text("message") }
}
