//! Every relayer call as a value: its method, path and body, made into a full request at any
//! base, the venue's origin or an app's proxy.

use df_hyperliquid_core::client::Method;
use df_hyperliquid_core::utils::Address;

use super::*;
use crate::client::deployment::ORIGIN;

const WALLET: &str = "0x9587c978341b4de61fa14e4705f65fbaaec72741";

#[test]
fn each_call_has_its_method_path_and_body() {
    // Checksummed in, lowercase out.
    let wallet: Address = "0x9587C978341B4DE61FA14E4705F65FBAAEC72741".parse().unwrap();
    for (call, method, path, body) in [
        (Call::trading_state(), Method::Get, "/state/trading".to_owned(), None),
        (Call::account_stream(wallet), Method::Get, format!("/state/user/live?wallet={WALLET}"), None),
        (Call::summary(), Method::Get, "/query/protocol/summary".to_owned(), None),
        (Call::market_stats(), Method::Get, "/query/protocol/market-stats".to_owned(), None),
        (Call::queue_summary(), Method::Get, "/query/protocol/queue-summary".to_owned(), None),
        (Call::recent_trades(), Method::Get, "/query/protocol/trades/recent".to_owned(), None),
        (Call::queue_rank(wallet), Method::Get, format!("/queue-rank/accounts/{WALLET}"), None),
        (Call::health(), Method::Get, "/query/health".to_owned(), None),
        (
            Call::deposit_check(wallet),
            Method::Post,
            "/funding/deposits/check".to_owned(),
            Some(format!(r#"{{"user":"{WALLET}"}}"#)),
        ),
    ] {
        assert_eq!(call, Call { method, path, body });
    }
}

#[test]
fn a_call_is_a_request_at_any_base() {
    let json = ("Accept".to_owned(), "application/json".to_owned());
    let typed = ("Content-Type".to_owned(), "application/json".to_owned());
    let wallet: Address = WALLET.parse().unwrap();
    let rank = format!("/api/queue-rank/accounts/{WALLET}");
    for (base, call, url, headers) in [
        (ORIGIN, Call::trading_state(), "https://exchange.papertrade.xyz/state/trading", vec![json.clone()]),
        ("/api/", Call::trading_state(), "/api/state/trading", vec![json.clone()]),
        ("/api", Call::queue_rank(wallet), rank.as_str(), vec![json.clone()]),
        (
            ORIGIN,
            Call::deposit_check(wallet),
            "https://exchange.papertrade.xyz/funding/deposits/check",
            vec![json.clone(), typed.clone()],
        ),
    ] {
        let request = call.request(base);
        assert_eq!((request.method, request.url.as_str(), &request.headers), (call.method, url, &headers));
        assert_eq!(request.body, call.body.clone().map(String::into_bytes).unwrap_or_default(), "{url}");
    }
}
