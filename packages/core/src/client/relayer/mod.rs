//! The relayer: requests as pure values ([`Call`]), what it answers a submission ([`Accepted`]),
//! why a call failed ([`RelayerError`]), and the [`Relayer`] that sends them over
//! [`Http`]. Submissions are never retried: without an answer the intent may still have been
//! accepted, and the account stream shows whether it was.

pub mod accepted;
pub mod call;
pub mod error;

pub use accepted::Accepted;
pub use call::Call;
pub use error::RelayerError;

use df_hyperliquid_core::client::Http;
use df_hyperliquid_core::utils::{Json, hex};

use crate::client::deployment::ORIGIN;
use crate::trade::intent::{Intent, Signed};
use crate::utils::wire::BadWire;

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
            return Err(error::refusal(response.status, &response.body));
        }
        let text = String::from_utf8(response.body)
            .map_err(|_| BadWire { what: "body".to_owned(), found: "not UTF-8".to_owned() })?;
        // A 2xx can still carry a refusal: `{"ok":false}` or an `error` that is not `null`.
        if let Ok(json) = Json::parse(&text)
            && (json.get("error").is_some_and(|error| *error != Json::Null)
                || matches!(json.get("ok"), Some(Json::Bool(false))))
        {
            return Err(error::refusal(response.status, text.as_bytes()));
        }
        Ok(text)
    }

    /// Submits a signed intent once and checks the relayer accepted the same intent id. A
    /// cancel's answer is not checked.
    pub async fn submit(&self, signed: &Signed) -> Result<Accepted, RelayerError> {
        let accepted = Accepted::parse(&self.send(&Call::submit(signed)).await?)?;
        if matches!(signed.intent(), Intent::CancelIntent(_)) {
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
}
