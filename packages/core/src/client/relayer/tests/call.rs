//! A call is a full request at any base: the venue's origin or an app's proxy.

use df_hyperliquid_core::client::Method;
use df_hyperliquid_core::utils::Address;

use super::*;
use crate::client::deployment::ORIGIN;

#[test]
fn a_call_is_a_request_at_a_base() {
    let wallet: Address = "0x9587C978341B4DE61FA14E4705F65FBAAEC72741".parse().unwrap();
    let request = Call::account_stream(wallet).request("/api/");
    assert_eq!(request.url, "/api/state/user/live?wallet=0x9587c978341b4de61fa14e4705f65fbaaec72741");
    assert_eq!(request.method, Method::Get);
    assert!(request.body.is_empty());
    let check = Call::deposit_check(wallet).request(ORIGIN);
    assert_eq!(check.url, "https://exchange.papertrade.xyz/funding/deposits/check");
    assert_eq!(check.body, br#"{"user":"0x9587c978341b4de61fa14e4705f65fbaaec72741"}"#);
    assert!(check.headers.contains(&("Content-Type".to_owned(), "application/json".to_owned())));
    assert_eq!(Call::queue_rank(wallet).path, "/queue-rank/accounts/0x9587c978341b4de61fa14e4705f65fbaaec72741");
}
