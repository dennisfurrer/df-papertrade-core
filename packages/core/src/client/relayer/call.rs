//! Relayer requests as pure values: a method, a path from the root and an optional JSON body,
//! made into a full [`Request`] against any base URL.

use df_hyperliquid_core::client::{Method, Request};
use df_hyperliquid_core::utils::{Address, Json};

use crate::trade::intent::Signed;

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

    /// The full request against `base`, such as [`ORIGIN`](crate::client::deployment::ORIGIN)
    /// or `/api`.
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

#[cfg(test)]
#[path = "tests/call.rs"]
mod tests;
