use hl_core::Method;
use hl_papertrade::relayer::RelayerError;

use super::*;
use crate::Error;

#[test]
fn a_page_reaches_the_relayer_through_its_app() {
    let wire = Wire::default();
    let client = client(&wire);
    assert_eq!(client.config().relayer, RELAY);
    assert_eq!(client.stream_url(owner()), format!("{RELAY}/state/user/live?wallet={}", owner().to_hex()));
    assert!(Config::mainnet(now, sevens).relayer.starts_with("https://"));
}

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
