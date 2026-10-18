//! Reads over a scripted wire: the relayer's answers and HyperEVM's.

use df_hyperliquid_core::client::Method;

use super::*;
use crate::client::RelayerError;
use crate::test_support::{RPC, TRADING, Wire, block_on, body, client};

#[test]
fn the_trading_state_reads_through_the_wire() {
    let wire = Wire::default().on(Method::Get, "/api/papertrade/state/trading", "", 200, TRADING);
    let state = block_on(client(&wire).trading_state()).unwrap();
    assert_eq!(state.instruments.len(), 2);
    assert_eq!(wire.sent()[0].url, "/api/papertrade/state/trading");
}

#[test]
fn a_refusal_keeps_the_relayers_code() {
    let refused = r#"{"ok":false,"error":"deadline_expired","message":"expired"}"#;
    let wire = Wire::default().on(Method::Get, "/api/papertrade/state/trading", "", 422, refused);
    let e = block_on(client(&wire).trading_state()).unwrap_err();
    assert!(
        matches!(e, Error::Relayer(RelayerError::Refused { status: 422, code: Some(code), .. }) if code == "deadline_expired")
    );
}

/// The BBO precompile (`0x...080e`) is read for the market's perp index, and the quote is its mid.
#[test]
fn a_quote_is_the_books_mid() {
    let answer = format!(r#"{{"jsonrpc":"2.0","id":1,"result":"0x{:064x}{:064x}"}}"#, 829_100, 829_110);
    let wire = Wire::default().on(Method::Post, RPC, "000000000000000000000000000000000000080e", 200, &answer);
    let quote = block_on(client(&wire).quote(1)).unwrap();
    assert_eq!(quote, Quote { bid: 829_100, ask: 829_110, mid: 829_105 });
    assert_eq!(body(&wire.sent()[0])["params"][0]["data"], format!("0x{:064x}", 1));
}
